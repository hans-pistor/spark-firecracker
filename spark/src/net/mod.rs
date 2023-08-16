use std::process::Command;

#[derive(Debug)]
pub struct VmNetwork {
    pub tap_device_name: String,
}

impl VmNetwork {
    pub fn create(vm_id: usize, host_network_interface: &str) -> anyhow::Result<Self> {
        assert!(vm_id < 256);
        let tap_device_name = format!("fc-tap{vm_id}");

        // Create the tap device
        let create_tap_device_args = format!("tuntap add {} mode tap", tap_device_name);
        Command::new("ip")
            .args(create_tap_device_args.split(' '))
            .output()?;

        let add_tap_device_to_bridge_args = format!("link set dev {tap_device_name} master br0");
        Command::new("ip")
            .args(add_tap_device_to_bridge_args.split(' '))
            .output()?;

        // Enables the new tap device
        let enable_tap_device_args = format!("link set {} up", tap_device_name);
        Command::new("ip")
            .args(enable_tap_device_args.split(' '))
            .output()?;

        Ok(Self { tap_device_name })
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
    pub fn new(host_iface: &str) -> anyhow::Result<Self> {
        let conntrack_args = "-A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT";
        Command::new("iptables")
            .args(conntrack_args.split(' '))
            .output()?;

        let nat_args = format!("-t nat -A POSTROUTING -o {host_iface} -j MASQUERADE");
        Command::new("iptables")
            .args(nat_args.split(' '))
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

pub struct BridgeNetwork {
    pub bridge_name: String,
    pub ip_address: String,
}

impl BridgeNetwork {
    pub fn new(bridge_name: &str, ip_address: &str) -> anyhow::Result<Self> {
        let create_bridge_args = format!("link add name {} type bridge", bridge_name);
        Command::new("ip")
            .args(create_bridge_args.split(' '))
            .output()?;

        let create_ip_address_args = format!("addr add {}/24 dev {}", ip_address, bridge_name);
        Command::new("ip")
            .args(create_ip_address_args.split(' '))
            .output()?;

        let enable_bridge_args = format!("link set {} up", bridge_name);
        Command::new("ip")
            .args(enable_bridge_args.split(' '))
            .output()?;

        Ok(Self {
            bridge_name: bridge_name.to_string(),
            ip_address: ip_address.to_string(),
        })
    }
}

impl Drop for BridgeNetwork {
    fn drop(&mut self) {
        let delete_bridge_args = format!("link del {}", self.bridge_name);
        Command::new("ip")
            .args(delete_bridge_args.split(' '))
            .output()
            .expect("Failed to delete the bridge");
    }
}
