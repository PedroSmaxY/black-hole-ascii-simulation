#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("ProductName", "Black Hole ASCII");
    res.set("FileDescription", "Real-Time 4D Kerr-Schild Relativistic Black Hole Simulator");
    res.set("LegalCopyright", "Copyright (c) 2026 PedroSmaxY");
    if let Err(e) = res.compile() {
        eprintln!("Warning: failed to compile Windows resources: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {}
