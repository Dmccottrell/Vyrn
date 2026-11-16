// Gives the shared library a name that works wherever it is copied: the
// Unreal plugin ships it next to the game, not where Cargo built it.
fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {
            println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libvyrn.dylib")
        }
        Ok("linux") => println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libvyrn.so"),
        _ => {}
    }
}
