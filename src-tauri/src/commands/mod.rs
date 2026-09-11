// 桌面专属命令（原生对话框）仅随 desktop feature 编译；
// fonts / wallpaper 为纯 Rust 实现，桌面命令与 Web HTTP 路由共用。
#[cfg(feature = "desktop")]
pub mod dialog;
pub mod fonts;
pub mod wallpaper;
