fn main() {
    // Windows 本地终端依赖随包侧载的官方 ConPTY（见 resources/conpty/README.md）。
    // 仅目标平台为 Windows 时复制；并按目标架构选择变体目录
    // （resources/conpty/x86_64 | aarch64，来自官方 NuGet 的 x64 / arm64）。
    //
    // portable-pty 用 LoadLibrary 加载 exe 同目录的 conpty.dll（默认先搜应用目录），
    // conpty.dll 再从自身目录拉起 OpenConsole.exe，因此两个文件必须与 exe 同目录：
    // - 开发/普通构建：直接复制到产物 exe 目录（兼容有无 --target 的 OUT_DIR 布局）
    // - tauri 打包：资源映射无法按架构分支，故先以固定名暂存到
    //   resources/conpty/bin/（已 gitignore），由 tauri.windows.conf.json 映射到安装根目录
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let variant = match arch.as_str() {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            other => panic!("resources/conpty 未提供 {other} 架构的 ConPTY 变体（现有 x86_64/aarch64）"),
        };
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let src_dir = std::path::Path::new(&manifest_dir)
            .join("resources/conpty")
            .join(variant);

        // 1) 复制到产物 exe 目录：OUT_DIR = target[/triple]/<profile>/build/<pkg>-<hash>/out，
        //    向上找到名为 <profile>（PROFILE，debug/release）的目录即 exe 所在目录
        let profile = std::env::var("PROFILE").expect("PROFILE");
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
        let exe_dir = std::path::Path::new(&out_dir)
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == profile.as_str()))
            .expect("OUT_DIR 结构异常：未找到 profile 目录")
            .to_path_buf();
        for file in ["conpty.dll", "OpenConsole.exe"] {
            let src = src_dir.join(file);
            let dst = exe_dir.join(file);
            if let Err(e) = std::fs::copy(&src, &dst) {
                panic!("复制 {file} 到 {} 失败: {e}", dst.display());
            }
            println!("cargo:rerun-if-changed=resources/conpty/{variant}/{file}");
        }

        // 2) 固定名暂存，供 tauri.windows.conf.json 的 resources 映射进安装包
        let stage = std::path::Path::new(&manifest_dir).join("resources/conpty/bin");
        std::fs::create_dir_all(&stage).expect("创建暂存目录");
        for file in ["conpty.dll", "OpenConsole.exe"] {
            let src = src_dir.join(file);
            let dst = stage.join(file);
            if let Err(e) = std::fs::copy(&src, &dst) {
                panic!("暂存 {file} 到 {} 失败: {e}", dst.display());
            }
        }
    }

    // tauri-build 负责桌面壳的资源/上下文代码生成；
    // Web server 目标（--no-default-features）不启用 desktop feature，跳过之
    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        tauri_build::build();
    }
}
