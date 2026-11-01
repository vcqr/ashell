//! 资源管理器拖文件到终端 → 填充路径。
//!
//! WebView2 的 HTML5 drop 事件里 File 对象拿不到完整路径（File.path 是 Electron
//! 专有），但 SDK 1.0.1777（2023-04 stable，WebView2Feedback#501）提供了宿主侧
//! 通道：页面把 drop 拿到的 File 经 window.chrome.webview.
//! postMessageWithAdditionalObjects() 传给宿主，宿主在 WebMessageReceived 的
//! AdditionalObjects 里能拿到 ICoreWebView2File::Path。
//!
//! 注意：实测本 WebView2 只对**字符串**消息派发 WebMessageReceived 事件
//! （对象消息体静默丢弃），所以消息体用 JSON.stringify 的字符串，Rust 侧二次解析。
//!
//! 数据流：
//!   终端 drop 事件(带 clientX/Y)
//!   → JS postMessageWithAdditionalObjects(JSON.stringify({kind,x,y}), files)
//!   → 本模块的 WebMessageReceived 处理器收集路径
//!   → webview.eval() 派发回页面 → 元素定位派发 ashell-drop CustomEvent
//!   → TerminalView 复用 pasteToTerminal 粘贴
//!
//! 仅 Windows 有此 API；其它平台命令为空实现（前端同样有守卫，行为维持现状）。
//! wry 自身的 WebMessageReceived 处理器与本处理器追加共存，互不影响。

/// 前端消息里的协议标记，避免误处理 ipc / 事件等其它 postMessage
const BRIDGE_KIND: &str = "ashell-file-drop";

/// 非 Windows（macOS/Linux 桌面壳）：保持命令存在，行为为空（维持现状）。
#[cfg(all(not(windows), feature = "desktop"))]
#[tauri::command]
pub fn install_file_drop_bridge() -> Result<(), String> {
    Ok(())
}

#[cfg(all(windows, feature = "desktop"))]
#[tauri::command]
pub fn install_file_drop_bridge(webview: tauri::Webview) -> Result<(), String> {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    // 防重复安装：同一 webview 重复 invoke 会注册多个处理器，拖一次发多遍
    static INSTALLED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let installed = INSTALLED.get_or_init(|| Mutex::new(HashSet::new()));
    let label = webview.label().to_string();
    if !installed.lock().expect("file drop lock").insert(label.clone()) {
        return Ok(());
    }

    let wv = webview.clone();
    let result = webview.with_webview(move |platform| {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs2,
        };
        use webview2_com::{take_pwstr, WebMessageReceivedEventHandler};
        use windows::core::{Interface, PWSTR};

        let Ok(web) = (unsafe { platform.controller().CoreWebView2() }) else {
            log::warn!("file drop bridge: CoreWebView2() failed");
            return;
        };

        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            // 整个闭包体都是 COM 调用，统一进 unsafe
            unsafe {
                let Some(args) = args else {
                    return Ok(());
                };

                // 消息体是 JSON.stringify 的字符串，WebMessageAsJson 再包一层 JSON
                // 引号；非本协议的消息（ipc 字符串/对象）解析不出来就放行
                let raw = {
                    let mut json = PWSTR::null();
                    if args.WebMessageAsJson(&mut json).is_err() {
                        return Ok(());
                    }
                    take_pwstr(json)
                };
                let parsed = serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|v| match v {
                        serde_json::Value::String(s) => {
                            serde_json::from_str::<serde_json::Value>(&s).ok()
                        }
                        obj @ serde_json::Value::Object(_) => Some(obj),
                        _ => None,
                    });
                let Some(v) = parsed else {
                    return Ok(());
                };
                if v.get("kind").and_then(|k| k.as_str()) != Some(BRIDGE_KIND) {
                    return Ok(());
                }
                let (Some(x), Some(y)) = (
                    v.get("x").and_then(serde_json::Value::as_f64),
                    v.get("y").and_then(serde_json::Value::as_f64),
                ) else {
                    return Ok(());
                };

                // 取拖入对象的完整磁盘路径（非 File 的对象跳过）
                let Ok(args2) = args.cast::<ICoreWebView2WebMessageReceivedEventArgs2>() else {
                    return Ok(());
                };
                let Ok(collection) = args2.AdditionalObjects() else {
                    return Ok(());
                };
                let mut count = 0u32;
                if collection.Count(&mut count).is_err() || count == 0 {
                    return Ok(());
                }
                let mut paths = Vec::with_capacity(count as usize);
                for i in 0..count {
                    let Ok(obj) = collection.GetValueAtIndex(i) else {
                        continue;
                    };
                    let Ok(file) = obj.cast::<ICoreWebView2File>() else {
                        continue;
                    };
                    let mut path = PWSTR::null();
                    if file.Path(&mut path).is_ok() {
                        let path = take_pwstr(path);
                        if !path.is_empty() {
                            paths.push(path);
                        }
                    }
                }
                if paths.is_empty() {
                    return Ok(());
                }

                // 派发回发起 drop 的 webview：由页面全局派发器定位元素并发 CustomEvent
                let payload = serde_json::json!({ "x": x, "y": y, "paths": paths });
                if let Err(e) =
                    wv.eval(format!("window.__ashellFileDrop&&window.__ashellFileDrop({payload})"))
                {
                    log::warn!("file drop eval: {e}");
                }
                Ok(())
            }
        }));
        let mut token = 0i64;
        if let Err(e) = unsafe { web.add_WebMessageReceived(&handler, &mut token) } {
            log::warn!("add_WebMessageReceived(file drop bridge): {e}");
        }
    });

    match result {
        Ok(()) => Ok(()),
        // 安装失败时移除标记，允许前端重试
        Err(e) => {
            installed.lock().expect("file drop lock").remove(&label);
            Err(e.to_string())
        }
    }
}
