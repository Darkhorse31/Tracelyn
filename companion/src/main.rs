#[cfg(unix)]
mod daemon;
fn main() {
    #[cfg(unix)]
    if let Err(error) = daemon::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    {
        eprintln!(
            "The companion runs on Linux. Build the musl release using scripts/build-companion.ps1"
        );
        std::process::exit(1);
    }
}
