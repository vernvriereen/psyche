use std::process::Command;
use tokio::signal;

pub struct DockerTestCleanup;
impl Drop for DockerTestCleanup {
    fn drop(&mut self) {
        println!("\nStopping containers...");
        let output = Command::new("docker")
            .args(["compose", "--profile", "all", "stop"])
            .output()
            .expect("Failed stop docker compose instances");

        if !output.status.success() {
            eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

pub fn e2e_testing_setup(init_num_clients: usize) -> DockerTestCleanup {
    spawn_psyche_network(init_num_clients);
    spawn_ctrl_c_task();

    DockerTestCleanup {}
}

pub fn spawn_psyche_network(init_num_clients: usize) {
    let output = Command::new("just")
        .args(["setup_test_infra", &format!("{}", init_num_clients)])
        .output()
        .expect("Failed spawn docker compose command");

    if !output.status.success() {
        eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("[+] Docker compose network spawned successfully!");
    println!();
}

pub fn spawn_ctrl_c_task() {
    tokio::spawn(async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        println!("\nCtrl+C received. Stopping containers...");
        let output = Command::new("docker")
            .args(["compose", "--profile", "all", "stop"])
            .output()
            .expect("Failed stop docker compose instances");

        if !output.status.success() {
            eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
        }
        std::process::exit(0);
    });
}
