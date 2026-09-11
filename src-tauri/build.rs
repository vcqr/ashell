fn main() {
    // tauri-build 负责桌面壳的资源/上下文代码生成；
    // Web server 目标（--no-default-features）不启用 desktop feature，跳过之
    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        tauri_build::build();
    }
}
