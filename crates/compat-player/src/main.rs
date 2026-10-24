fn main() {
    if let Err(error) = compat_player::run_worker() {
        eprintln!("compat-player: {error}");
        std::process::exit(1);
    }
}
