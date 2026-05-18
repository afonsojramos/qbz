//! Compiles the Slint UI tree. `ui/app.slint` is the single entry point; it
//! imports everything else, so only the entry file is listed here.

fn main() {
    slint_build::compile("ui/app.slint").expect("Slint UI failed to compile");
}
