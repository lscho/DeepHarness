// 防止 Windows 发布版启动时弹出并驻留 CMD 控制台窗口（勿删除）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    deepharness_lib::run();
}
