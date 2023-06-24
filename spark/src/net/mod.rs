use std::process::Command;

pub struct VmNetwork {
    pub tap_device_name: String,
    pub ip_address: String,
}

impl VmNetwork {
    pub fn create(vm_id: usize, host_network_interface: &str) -> anyhow::Result<Self> {
        assert!(vm_id < 256);
        let tap_device_name = format!("fc-tap{vm_id}");
        let ip_address = format!("172.16.{vm_id}.1/24");

        // Create the tap device
        let create_tap_device_args = format!("tuntap add {} mode tap", tap_device_name);
        Command::new("ip")
            .args(create_tap_device_args.split(' '))
            .output()?;

        // Create an IP address for the tap device
        let create_ip_address_args = format!("addr add {} dev {}", ip_address, tap_device_name);
        Command::new("ip")
            .args(create_ip_address_args.split(' '))
            .output()?;

        // Enables the new tap device
        let enable_tap_device_args = format!("link set {} up", tap_device_name);
        Command::new("ip")
            .args(enable_tap_device_args.split(' '))
            .output()?;

        // let nat_setup_for_vm_args =
        //     format!("-t nat -A POSTROUTING -o {host_network_interface} -j MASQUERADE");
        // Command::new("iptables")
        //     .args(nat_setup_for_vm_args.split(' '))
        //     .output()?;

        // When received from tap device, forward it to the host network
        // interface
        let forward_to_host_args = format!(
            "-A FORWARD -i {} -o {} -j ACCCEPT",
            tap_device_name, host_network_interface
        );
        Command::new("iptables")
            .args(forward_to_host_args.split(' '))
            .output()?;

        Ok(Self {
            tap_device_name,
            ip_address,
        })
    }
}

impl Drop for VmNetwork {
    fn drop(&mut self) {
        // sudo ip link del $TAP_DEVICE 2> /dev/null || true
        let delete_tap_device_args = format!("link del {}", self.tap_device_name);
        Command::new("ip")
            .args(delete_tap_device_args.split(' '))
            .output()
            .expect("Failed to delete the tap device");
    }
}

/// Flushes ip tables on drop
pub struct IpTablesGuard;

impl IpTablesGuard {
    pub fn new() -> anyhow::Result<Self> {
        let conntrack_args = "-A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT";
        Command::new("iptables")
            .args(conntrack_args.split(' '))
            .output()?;
        Ok(Self {})
    }
}
impl Drop for IpTablesGuard {
    fn drop(&mut self) {
        Command::new("iptables")
            .args(["-F"])
            .output()
            .expect("Failed to flush iptables");
    }
}
