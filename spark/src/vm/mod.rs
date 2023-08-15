use std::{
    marker::PhantomData,
    process::{Child, Command},
};

use anyhow::Context;
use hyper::{Client, Request};
use tokio::fs::try_exists;

use crate::net::VmNetwork;

use self::models::{VmBootSource, VmDrive, VmLogger, VmNetworkInterface};
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

pub mod models;

pub struct VmNotStarted;
pub struct VmStarted;
pub trait FirecrackerState {}
impl FirecrackerState for VmNotStarted {}
impl FirecrackerState for VmStarted {}

#[derive(Debug)]
struct VmState {
    vm_id: usize,
    data_directory: String,
    _vm_network: Option<VmNetwork>,
}

impl VmState {
    pub fn firecracker_socket_path(&self) -> String {
        format!("{}/firecracker.socket", self.data_directory)
    }

    pub fn firecracker_logger_path(&self) -> String {
        format!("{}/firecracker.log", self.data_directory)
    }
}

pub struct VirtualMachine<T: FirecrackerState> {
    vm_state: VmState,
    firecracker_process: Child,
    firecracker_client: Client<UnixConnector>,
    marker: PhantomData<T>,
}

impl VirtualMachine<VmNotStarted> {
    pub async fn new(firecracker_path: &str, vm_id: usize) -> anyhow::Result<Self> {
        let data_directory_path = format!("/tmp/vm/{vm_id}");
        if try_exists(&data_directory_path).await? {
            tokio::fs::remove_dir_all(&data_directory_path).await?;
        }
        tokio::fs::create_dir_all(&data_directory_path).await?;

        let data_directory = VmState {
            vm_id,
            data_directory: data_directory_path,
            _vm_network: None,
        };

        let firecracker_process = Command::new(firecracker_path)
            .arg("--api-sock")
            .arg(&data_directory.firecracker_socket_path())
            .spawn()?;

        // Wait for the firecracker socket to appear
        tokio::time::timeout(std::time::Duration::from_millis(500), async {
            while !tokio::fs::try_exists(&data_directory.firecracker_socket_path()).await? {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }

            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("Firecracker socket did not appear within 500ms")?
        .context("Failed to check if the firecracker socket path exists")?;

        Ok(Self {
            vm_state: data_directory,
            firecracker_process,
            firecracker_client: Client::unix(),
            marker: PhantomData,
        })
    }

    pub async fn with_logger(self) -> anyhow::Result<Self> {
        std::fs::File::create(self.vm_state.firecracker_logger_path())?;

        let logger = VmLogger {
            log_path: self.vm_state.firecracker_logger_path(),
            level: "Debug".into(),
            show_level: true,
            show_log_origin: true,
        };
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(self.vm_state.firecracker_socket_path(), "/logger"))
            .body(serde_json::to_string(&logger)?.into())?;

        self.firecracker_client.request(request).await?;
        Ok(self)
    }

    pub async fn setup_boot_source(self, boot_source: VmBootSource) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.vm_state.firecracker_socket_path(),
                "/boot-source",
            ))
            .body(serde_json::to_string(&boot_source)?.into())?;

        self.firecracker_client.request(request).await?;
        Ok(self)
    }

    pub async fn with_drive(self, drive: VmDrive) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.vm_state.firecracker_socket_path(),
                &format!("/drives/{}", drive.drive_id),
            ))
            .body(serde_json::to_string(&drive)?.into())?;

        self.firecracker_client.request(request).await?;

        Ok(self)
    }

    pub async fn add_network_interface(self, host_network_interface: &str) -> anyhow::Result<Self> {
        let vm_network = VmNetwork::create(self.vm_state.vm_id, host_network_interface)?;
        let vm_network_interface = VmNetworkInterface {
            iface_id: "eth0".into(),
            guest_mac: "AA:FC:00:00:00:01".into(),
            host_dev_name: vm_network.tap_device_name.clone(),
        };

        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.vm_state.firecracker_socket_path(),
                &format!("/network-interfaces/{}", vm_network_interface.iface_id),
            ))
            .body(serde_json::to_string(&vm_network_interface)?.into())?;

        self.firecracker_client.request(request).await?;

        Ok(Self {
            vm_state: VmState {
                _vm_network: Some(vm_network),
                ..self.vm_state
            },
            ..self
        })
    }

    pub async fn start(self) -> anyhow::Result<VirtualMachine<VmStarted>> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.vm_state.firecracker_socket_path(),
                "/actions",
            ))
            .body(
                serde_json::json!({"action_type": "InstanceStart"})
                    .to_string()
                    .into(),
            )?;

        self.firecracker_client.request(request).await?;
        Ok(VirtualMachine {
            vm_state: self.vm_state,
            firecracker_process: self.firecracker_process,
            firecracker_client: self.firecracker_client,
            marker: PhantomData,
        })
    }
}
