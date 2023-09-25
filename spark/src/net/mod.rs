use std::{
    ffi::{OsStr, OsString},
    process::Command,
};

use crate::cmd::{CommandNamespace, self};

#[derive(Debug, Clone)]
pub struct VmNetwork {
    pub tap_device_name: String,
}

impl VmNetwork {
    pub fn create(vm_id: usize, host_network_interface: &str, namespace: &CommandNamespace) -> anyhow::Result<Self> {
        assert!(vm_id < 256);
        let tap_device_name = format!("fc-tap{vm_id}");

        // Create the tap device
        cmd::run(
            namespace,
            "ip",
            format!("tuntap add {} mode tap", tap_device_name).split(' '),
        )?;

        cmd::run(
            &namespace,
            "ip",
            format!("addr add 172.16.0.1/24 dev {tap_device_name}").split(' '),
        )?;

        cmd::run(
            &namespace,
            "iptables",
            format!("-A FORWARD -i {tap_device_name} -o {host_network_interface} -j ACCEPT")
                .split(' '),
        )?;

        // Makes the interface a port in the bridge network `br0`
        // run(
        //     "ip",
        //     format!("link set dev {tap_device_name} master br0").split(' '),
        // )?;
        // Enables the new tap device
        cmd::run(&namespace, "ip", format!("link set {} up", tap_device_name).split(' '))?;


        Ok(Self { tap_device_name })
    }
}

impl Drop for VmNetwork {
    fn drop(&mut self) {
        // TAP device gets dropped when network namespace gets dropped.
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

// pub struct BridgeNetwork {
//     pub bridge_name: String,
//     pub ip_address: String,
// }

// impl BridgeNetwork {
//     pub fn new(bridge_name: &str, ip_address: &str) -> anyhow::Result<Self> {
//         // Create bridge network
//         run(
//             "ip",
//             format!("link add name {} type bridge", bridge_name).split(' '),
//         )?;

//         // Assign bridge network an IP address
//         run(
//             "ip",
//             format!("addr add {}/24 dev {}", ip_address, bridge_name).split(' '),
//         )?;

//         // enable the bridge network
//         run("ip", format!("link set {} up", bridge_name).split(' '))?;

//         Ok(Self {
//             bridge_name: bridge_name.to_string(),
//             ip_address: ip_address.to_string(),
//         })
//     }
// }

// impl Drop for BridgeNetwork {
//     fn drop(&mut self) {
//         // Delete the bridge network
//         run("ip", format!("link del {}", self.bridge_name).split(' '))
//             .expect("Failed to delete the bridge network");
//     }
// }
