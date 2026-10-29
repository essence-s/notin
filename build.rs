// Compila la UI de Slint en tiempo de build.
fn main() {
    slint_build::compile("ui/app.slint").expect("compila la UI");
}
