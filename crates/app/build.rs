//! Gives the Windows executable its icon.

fn main() {
    println!("cargo::rerun-if-changed=../../packaging/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("../../packaging/icon.ico")
            .compile()
            .expect("couldn't embed the Windows icon");
    }
}
