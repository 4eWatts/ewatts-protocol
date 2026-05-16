pub mod constants;
pub mod dag;
pub mod proof;
pub mod commitment;
pub mod vr;
pub mod block;
pub mod reward;
pub mod difficulty;
pub mod state;

fn main() {
    println!("Ewatts Protocol v{}", crate::constants::PROTOCOL_VERSION);
}
