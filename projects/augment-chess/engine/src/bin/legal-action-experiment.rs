//! User-run legal action experiment. No measurements run at build time.
fn main() {
    if let Err(error) = augment_chess_engine::legal_experiment::run_cli(std::env::args().skip(1)) {
        eprintln!("legal-action-experiment: {error}");
        std::process::exit(1);
    }
}
