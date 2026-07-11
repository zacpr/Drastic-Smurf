slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = HelloWindow::new()?;
    window.run()
}
