use env_logger;
use log::info;

fn main() {
    env_logger::init();

    info!("Maroon Node started");

    loop {
        info!("Maroon Node heartbeat");
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
