use std::{
    marker::PhantomData,
    process::{Child, Command},
};

use hyper::{Client, Request};
use tokio::fs::try_exists;

use self::models::{VmBootSource, VmDrive, VmLogger, VmNetworkInterface};
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

pub mod models;

pub struct VmNotStarted;
pub struct VmStarted;
pub trait VmState {}
impl VmState for VmNotStarted {}
impl VmState for VmStarted {}

struct VmDataDirectory {
    path: String,
}

impl VmDataDirectory {
    pub fn firecracker_socket_path(&self) -> String {
        format!("{}/firecracker.socket", self.path)
    }

    pub fn firecracker_logger_path(&self) -> String {
        format!("{}/firecracker.log", self.path)
    }
}

pub struct VirtualMachine<T: VmState> {
    data_directory: VmDataDirectory,
    firecracker_process: Child,
    firecracker_client: Client<UnixConnector>,
    marker: PhantomData<T>,
}

impl VirtualMachine<VmNotStarted> {
    pub async fn new(firecracker_path: String, vm_id: usize) -> anyhow::Result<Self> {
        let data_directory_path = format!("/tmp/vm/{vm_id}");
        if try_exists(&data_directory_path).await? {
            tokio::fs::remove_dir_all(&data_directory_path).await?;
        }
        tokio::fs::create_dir_all(&data_directory_path).await?;

        let data_directory = VmDataDirectory {
            path: data_directory_path,
        };

        let firecracker_process = Command::new(firecracker_path)
            .arg("--api-sock")
            .arg(&data_directory.firecracker_socket_path())
            .spawn()?;

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Ok(Self {
            data_directory,
            firecracker_process,
            firecracker_client: Client::unix(),
            marker: PhantomData,
        })
    }

    pub async fn with_logger(self) -> anyhow::Result<Self> {
        std::fs::File::create(self.data_directory.firecracker_logger_path())?;

        let logger = VmLogger {
            log_path: self.data_directory.firecracker_logger_path(),
            level: "Debug".into(),
            show_level: true,
            show_log_origin: true,
        };
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.data_directory.firecracker_socket_path(),
                "/logger",
            ))
            .body(serde_json::to_string(&logger)?.into())?;

        self.firecracker_client.request(request).await?;
        Ok(self)
    }

    pub async fn setup_boot_source(self, boot_source: VmBootSource) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.data_directory.firecracker_socket_path(),
                "/boot-source",
            ))
            .body(serde_json::to_string(&boot_source)?.into())?;

        self.firecracker_client.request(request).await?;
        Ok(self)
    }

    pub async fn setup_root_fs(self, root_fs: VmDrive) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.data_directory.firecracker_socket_path(),
                &format!("/drives/{}", root_fs.drive_id),
            ))
            .body(serde_json::to_string(&root_fs)?.into())?;

        self.firecracker_client.request(request).await?;

        Ok(self)
    }

    pub async fn add_network_interface(
        self,
        network_interface: VmNetworkInterface,
    ) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.data_directory.firecracker_socket_path(),
                &format!("/network-interfaces/{}", network_interface.iface_id),
            ))
            .body(serde_json::to_string(&network_interface)?.into())?;

        self.firecracker_client.request(request).await?;

        Ok(self)
    }

    pub async fn start(self) -> anyhow::Result<VirtualMachine<VmStarted>> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.data_directory.firecracker_socket_path(),
                "/actions",
            ))
            .body(
                serde_json::json!({"action_type": "InstanceStart"})
                    .to_string()
                    .into(),
            )?;

        self.firecracker_client.request(request).await?;
        Ok(VirtualMachine {
            data_directory: self.data_directory,
            firecracker_process: self.firecracker_process,
            firecracker_client: self.firecracker_client,
            marker: PhantomData,
        })
    }
}
