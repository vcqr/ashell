fn main() {
    // Windows 本地终端依赖随包侧载的官方 ConPTY（见 resources/conpty/README.md）：
    // portable-pty 加载 exe 同目录的 conpty.dll（LoadLibrary 默认先搜应用目录），
    // conpty.dll 再从自身目录拉起 OpenConsole.exe，因此两个文件都必须与 exe 同目录。
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
        // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out，向上回溯到 target 目录
        let target_dir = std::path::Path::new(&out_dir)
            .ancestors()
            .nth(3)
            .expect("OUT_DIR 结构异常")
            .to_path_buf();
        for file in ["conpty.dll", "OpenConsole.exe"] {
            let src = std::path::Path::new(&manifest_dir).join("resources/conpty").join(file);
            let dst = target_dir.join(file);
            if src != dst {
                if let Err(e) = std::fs::copy(&src, &dst) {
                    panic!("复制 {file} 到 {} 失败: {e}", dst.display());
                }
            }
            println!("cargo:rerun-if-changed=resources/conpty/{file}");
        }
    }

    // tauri-build 负责桌面壳的资源/上下文代码生成；
    // Web server 目标（--no-default-features）不启用 desktop feature，跳过之
    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        tauri_build::build();
    }
}
