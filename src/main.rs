fn main() {
    env_logger::init();
    if let Err(e) = gitoxide_fs::cli::run() {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}
