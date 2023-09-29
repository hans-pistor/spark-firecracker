use std::{
    marker::PhantomData,
    path::PathBuf,
    process::{Child, Command},
    sync::Arc,
};
use std::io::Write;

use anyhow::Context;
use hyper::{Client, Request};
use tokio::fs::try_exists;
use userfaultfd::UffdBuilder;
use uuid::Uuid;

use crate::{
    cmd::{self, CommandNamespace},
    net::VmNetwork,
};

use self::models::{
    LoadSnapshotRequest, VmBootSource, VmDrive, VmLogger, VmNetworkInterface, VmSnapshotRequest, BackendType,
};
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

pub mod models;

pub const GUEST_INTERFACE: &str = "eth0";

pub struct VmNotStarted;
pub struct VmStarted;
pub struct VmPaused;
pub trait FirecrackerState {}
impl FirecrackerState for VmNotStarted {}
impl FirecrackerState for VmStarted {}
impl FirecrackerState for VmPaused {}

#[derive(Debug, Clone)]
pub struct VmState {
    pub vm_id: Uuid,
    pub data_directory: PathBuf,

    pub network_namespace: CommandNamespace,
    pub boot_source: Option<VmBootSource>,
    pub _vm_network: Option<VmNetwork>,
}

impl VmState {
    pub fn firecracker_socket_path(&self) -> PathBuf {
        self.data_directory.join("firecracker.socket")
    }

    pub fn firecracker_logger_path(&self) -> PathBuf {
        self.data_directory.join("firecracker.log")
    }

    pub fn network_namespace(&self) -> &CommandNamespace {
        &self.network_namespace
    }
}

impl Drop for VmState {
    fn drop(&mut self) {
        if let CommandNamespace::Named(namespace) = &self.network_namespace {
            cmd::run(
                &CommandNamespace::Global,
                "ip",
                format!("netns del {namespace}").split(' '),
            )
            .expect("failed to delete namespace");
        }
    }
}

#[derive(Clone)]
pub struct VirtualMachine<T: FirecrackerState> {
    pub vm_state: VmState,
    firecracker_process: Arc<Child>,
    firecracker_client: Client<UnixConnector>,
    marker: PhantomData<T>,
}

impl VirtualMachine<VmNotStarted> {
    pub async fn new(firecracker_path: &str, vm_id: Uuid) -> anyhow::Result<Self> {
        let data_directory_path: PathBuf = format!("/tmp/vm/{vm_id}").into();
        if try_exists(&data_directory_path).await? {
            tokio::fs::remove_dir_all(&data_directory_path).await?;
        }
        tokio::fs::create_dir_all(&data_directory_path).await?;

        let namespace = CommandNamespace::Named(format!("fc-{}", vm_id.to_string()));
        println!("Creating network namespace {namespace}");
        // Create network namespace
        cmd::run(
            &CommandNamespace::Global,
            "ip",
            format!("netns add {namespace}").split(' '),
        )?;

        let vm_state = VmState {
            vm_id,
            data_directory: data_directory_path,
            network_namespace: namespace,
            boot_source: None,
            _vm_network: None,
        };

        let firecracker_arguments = match &vm_state.network_namespace {
            CommandNamespace::Global => (
                firecracker_path,
                format!(
                    "--api-sock {}",
                    &vm_state.firecracker_socket_path().to_string_lossy()
                ),
            ),
            CommandNamespace::Named(ns) => (
                "ip",
                format!(
                    "netns exec {ns} {firecracker_path} --api-sock {}",
                    &vm_state.firecracker_socket_path().to_string_lossy()
                ),
            ),
        };
        let firecracker_process = Command::new(firecracker_arguments.0)
            .args(firecracker_arguments.1.split(' '))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?;

        // Wait for the firecracker socket to appear
        tokio::time::timeout(std::time::Duration::from_millis(500), async {
            while !tokio::fs::try_exists(&vm_state.firecracker_socket_path()).await? {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }

            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("Firecracker socket did not appear within 500ms")?
        .context("Failed to check if the firecracker socket path exists")?;

        let state = Self {
            vm_state,
            firecracker_process: Arc::from(firecracker_process),
            firecracker_client: Client::unix(),
            marker: PhantomData,
        };

        Ok(state)
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

    pub async fn setup_boot_source(mut self, boot_source: VmBootSource) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.vm_state.firecracker_socket_path(),
                "/boot-source",
            ))
            .body(serde_json::to_string(&boot_source)?.into())?;

        self.firecracker_client.request(request).await?;
        self.vm_state.boot_source = Some(boot_source);

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

    pub async fn load_snapshot(
        mut self,
        host_network_interface: &str,
        load_snapshot_request: LoadSnapshotRequest,
    ) -> anyhow::Result<VirtualMachine<VmPaused>> {
        let vm_network =
            VmNetwork::create(host_network_interface, &self.vm_state.network_namespace)?;
        self.vm_state._vm_network = Some(vm_network);

        let load_snapshot_request = LoadSnapshotRequest {
            resume_vm: false,
            ..load_snapshot_request
        };

        if load_snapshot_request.mem_backend.backend_type == BackendType::Uffd {
            let uffd = UffdBuilder::new()
            .close_on_exec(true)
            .non_blocking(true)
            .user_mode_only(true)
            .create()
            .unwrap();
        }

        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.vm_state.firecracker_socket_path(),
                "/snapshot/load",
            ))
            .body(serde_json::to_string(&load_snapshot_request)?.into())?;

        self.firecracker_client.request(request).await?;

        Ok(VirtualMachine {
            vm_state: self.vm_state,
            firecracker_process: self.firecracker_process,
            firecracker_client: self.firecracker_client,
            marker: PhantomData,
        })
    }

    pub async fn add_network_interface(
        mut self,
        host_network_interface: &str,
        ip_address: &str,
        gateway_ip: &str,
    ) -> anyhow::Result<Self> {
        let vm_network =
            VmNetwork::create(host_network_interface, &self.vm_state.network_namespace)?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let bytes = &self.vm_state.vm_id.as_bytes()[0..4];
        let guest_mac = format!(
            "AA:FC:{:02x}:{:02x}:{:02x}:{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3]
        );
        println!("Guest mac: {guest_mac}");
        let vm_network_interface = VmNetworkInterface {
            iface_id: GUEST_INTERFACE.into(),
            guest_mac,
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

        self.vm_state._vm_network = Some(vm_network);

        match self.vm_state.boot_source.clone() {
            None => panic!("Boot source must be set before adding a network interface"),
            Some(boot_source) => {
                self.setup_boot_source(VmBootSource {
                    boot_args: format!(
                        "{} IP_ADDRESS::{} IFACE::{} GATEWAY::{}",
                        boot_source.boot_args, ip_address, GUEST_INTERFACE, gateway_ip
                    ),
                    ..boot_source
                })
                .await
            }
        }
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

impl VirtualMachine<VmStarted> {
    pub async fn pause_vm(self) -> anyhow::Result<VirtualMachine<VmPaused>> {
        let request = Request::builder()
            .method("PATCH")
            .uri(Uri::new(self.vm_state.firecracker_socket_path(), "/vm"))
            .body(serde_json::json!({"state": "Paused"}).to_string().into())?;

        self.firecracker_client.request(request).await?;

        Ok(VirtualMachine {
            vm_state: self.vm_state,
            firecracker_process: self.firecracker_process,
            firecracker_client: self.firecracker_client,
            marker: PhantomData,
        })
    }
}

impl VirtualMachine<VmPaused> {
    pub async fn snapshot_vm(self, snapshot_request: &VmSnapshotRequest) -> anyhow::Result<Self> {
        let request = Request::builder()
            .method("PUT")
            .uri(Uri::new(
                self.vm_state.firecracker_socket_path(),
                "/snapshot/create",
            ))
            .body(serde_json::to_string(snapshot_request)?.into())?;

        self.firecracker_client.request(request).await?;

        Ok(self)
    }

    pub async fn resume_vm(self) -> anyhow::Result<VirtualMachine<VmStarted>> {
        let request = Request::builder()
            .method("PATCH")
            .uri(Uri::new(self.vm_state.firecracker_socket_path(), "/vm"))
            .body(serde_json::json!({"state": "Resumed"}).to_string().into())?;

        self.firecracker_client.request(request).await?;

        Ok(VirtualMachine {
            vm_state: self.vm_state,
            firecracker_process: self.firecracker_process,
            firecracker_client: self.firecracker_client,
            marker: PhantomData,
        })
    }
}


pub struct MemoryMapping {
    /// Raw C-style pointer to the starting address for the memory mapping. Immutable value.
    addr: *mut u8,
    /// Length of the mapping. Immutable value.
    size: usize,
}

impl MemoryMapping {
    fn new_with_flags_and_fd(size: usize, fd: i32, flags: i32) -> MemoryMapping {
        let addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                flags | libc::MAP_NORESERVE,
                fd,
                0,
            )
        };
        if addr == libc::MAP_FAILED {
            panic!("{}", std::io::Error::last_os_error());
        }
        MemoryMapping {
            addr: addr as *mut u8,
            size,
        }
    }

    fn write_slice(&self, buf: &[u8], offset: usize) -> std::io::Result<usize> {
        unsafe {
            let mut slice: &mut [u8] =
                &mut std::slice::from_raw_parts_mut(self.addr, self.size)[offset..];
            slice.write(buf)
        }
    }

    fn read_slice(&self, mut buf: &mut [u8], offset: usize) -> std::io::Result<usize> {
        unsafe {
            let slice: &[u8] = &std::slice::from_raw_parts(self.addr, self.size)[offset..];
            buf.write(slice)
        }
    }
}