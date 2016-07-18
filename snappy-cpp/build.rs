extern crate pkg_config;

fn main() {
    println!("cargo:rustc-link-lib=dylib=snappy");
}
