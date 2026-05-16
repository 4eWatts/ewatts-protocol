pub mod constants;
pub mod dag;
pub mod proof;
pub mod commitment;
pub mod vr;
pub mod block;
pub mod reward;
pub mod difficulty;
pub mod state;
pub mod store;

fn main() {
    println!("Ewatts Protocol v{}", crate::constants::PROTOCOL_VERSION);
    if crate::store::has_data() {
        println!("Node has persistent data. Loading...");
    } else {
        println!("Fresh start. No data found.");
    }
}
