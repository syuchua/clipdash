#[cfg(feature = "gtk-ui")]
mod gtk_app;

#[cfg(feature = "gtk-ui")]
fn main() {
    if let Err(e) = gtk_app::run() {
        eprintln!("clipdash-ui error: {}", e);
    }
}

#[cfg(not(feature = "gtk-ui"))]
fn main() {
    eprintln!("clipdash-ui built without gtk-ui feature. Build with: cargo run -p clipdash-ui --features gtk-ui");
}
