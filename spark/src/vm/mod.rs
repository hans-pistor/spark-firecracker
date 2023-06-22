use std::{
    marker::PhantomData,
    process::{Child, Command},
};

use hyper::{Client, Request};

use self::models::{VmBootSource, VmDrive, VmNetworkInterface};
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

pub mod models;

pub struct VmNotStarted;
pub struct VmStarted;
pub trait VmState {}
impl VmState for VmNotStarted {}
impl VmState for VmStarted {}

pub struct VirtualMachine<T: VmState> {
    firecracker_process: Child,
    firecracker_socket: String,
    firecracker_client: Client<UnixConnector>,
    boot_source: Option<VmBootSource>,
    root_fs: Option<VmDrive>,
    network_interfaces: Vec<VmNetworkInterface>,
    marker: PhantomData<T>,
}

impl VirtualMachine<VmNotStarted> {
    pub async fn new(firecracker_path: String) -> anyhow::Result<Self> {
        let firecracker_socket = "/tmp/firecracker.socket";
        let firecracker_process = Command::new(firecracker_path)
            .arg("--api-sock")
            .arg(firecracker_socket)
            .spawn()?;

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Ok(Self {
            firecracker_process,
            firecracker_socket: firecracker_socket.into(),
            firecracker_client: Client::unix(),
            boot_source: None,
            root_fs: None,
            network_interfaces: Vec::new(),
            marker: PhantomData,
        })
    }

    pub async fn setup_boot_source(self, boot_source: VmBootSource) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(&self.firecracker_socket, "/boot-source"))
            .body(serde_json::to_string(&boot_source)?.into())?;

        self.firecracker_client.request(request).await?;
        Ok(Self {
            boot_source: Some(boot_source),
            ..self
        })
    }

    pub async fn setup_root_fs(self, root_fs: VmDrive) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                &self.firecracker_socket,
                &format!("/drives/{}", root_fs.drive_id),
            ))
            .body(serde_json::to_string(&root_fs)?.into())?;

        self.firecracker_client.request(request).await?;

        Ok(Self {
            root_fs: Some(root_fs),
            ..self
        })
    }

    pub async fn add_network_interface(
        mut self,
        network_interface: VmNetworkInterface,
    ) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                &self.firecracker_socket,
                &format!("/network-interfaces/{}", network_interface.iface_id),
            ))
            .body(serde_json::to_string(&network_interface)?.into())?;

        self.firecracker_client.request(request).await?;
        self.network_interfaces.push(network_interface);

        Ok(self)
    }

    pub async fn start(self) -> anyhow::Result<VirtualMachine<VmStarted>> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(&self.firecracker_socket, "/actions"))
            .body(
                serde_json::json!({"action_type": "InstanceStart"})
                    .to_string()
                    .into(),
            )?;

        self.firecracker_client.request(request).await?;
        Ok(VirtualMachine {
            firecracker_process: self.firecracker_process,
            firecracker_socket: self.firecracker_socket,
            firecracker_client: self.firecracker_client,
            boot_source: self.boot_source,
            root_fs: self.root_fs,
            network_interfaces: self.network_interfaces,
            marker: PhantomData,
        })
    }
}
