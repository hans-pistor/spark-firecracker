use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing, Json, Router,
};
use clap::Parser;
use hyper::StatusCode;
use netns_rs::{get_from_current_thread, NetNs};
use spark_lib::{
    api::{vm_actions_client::VmActionsClient, GetDmesgRequest, PingRequest, ShutdownRequest},
    cmd::CommandNamespace,
    net::IpTablesGuard,
    vm::{
        models::{LoadSnapshotRequest, SnapshotType, VmBootSource, VmDrive, VmSnapshotRequest},
        VirtualMachine, VmStarted,
    },
};
use tokio::signal::{self, unix::SignalKind};
use uuid::Uuid;

pub const BRIDGE_IP: &str = "172.16.0.1";

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
struct Args {
    #[arg(long, default_value = "ens33")]
    host_network_interface: String,

    #[arg(long)]
    firecracker_path: String,

    #[arg(long)]
    kernel_image_path: String,

    #[arg(
        long,
        default_value = "console=ttyS0 reboot=k panic=1 pci=off nomodules ipv6.disable=1 8250.nr_uarts=0  tsc=reliable quiet i8042.nokbd i8042.noaux"
    )]
    boot_args: String,

    #[arg(long)]
    root_fs_path: String,
}

#[derive(Clone)]
struct AppState {
    config: Args,
    vms: Arc<RwLock<HashMap<String, VirtualMachine<VmStarted>>>>,
}

impl AppState {
    fn read_vms(
        &self,
    ) -> anyhow::Result<RwLockReadGuard<'_, HashMap<String, VirtualMachine<VmStarted>>>> {
        self.vms
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to get a read lock on VMs"))
    }

    fn write_vms(
        &self,
    ) -> anyhow::Result<RwLockWriteGuard<'_, HashMap<String, VirtualMachine<VmStarted>>>> {
        self.vms
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to get a read lock on VMs"))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Args::parse();
    let state = AppState {
        config,
        vms: Arc::default(),
    };
    let _guard = IpTablesGuard::new(&state.config.host_network_interface)?;

    let app = Router::new()
        .route("/vms/:vmid/execute/:action", routing::put(execute_action))
        .route("/vms", routing::get(list_vms))
        .route("/vms/create", routing::put(create_vm))
        .route("/vms/resume", routing::put(resume_vm))
        .with_state(state);

    axum::Server::bind(&"0.0.0.0:3000".parse()?)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            let mut interrupt = signal::unix::signal(SignalKind::interrupt())
                .expect("Failed to install SIGINT handler");
            let mut terminate = signal::unix::signal(SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");

            let ctrl_c = async {
                signal::ctrl_c()
                    .await
                    .expect("failed to install Ctrl+C handler");
            };

            tokio::select! {
                _ = interrupt.recv() => {},
                _ = terminate.recv() => {}
                _ = ctrl_c => {}
            }
            println!("Shutting down the server");
        })
        .await?;

    Ok(())
}

async fn execute_action(
    State(state): State<AppState>,
    Path((vm_id, action)): Path<(String, String)>,
) -> Result<String, AppError> {
    let namespace = match state.read_vms()?.get(&vm_id) {
        Some(vm) => match vm.vm_state.network_namespace() {
            CommandNamespace::Named(name) => name.clone(),
            _ => unimplemented!(),
        },
        None => Err(anyhow::anyhow!("No VM with id {vm_id} present"))?,
    };
    let target_ns = NetNs::get(namespace)?;

    let src_ns = get_from_current_thread()?;
    if &src_ns != &target_ns {
        target_ns.enter()?;
    }

    let mut client = VmActionsClient::connect(format!("http://{}:{}", "172.16.0.2", 3000)).await?;

    let response = match action.as_str() {
        "ping" => {
            let request = tonic::Request::new(PingRequest {});
            let response = client.ping(request).await?;
            format!("{response:?}")
        }
        "shutdown" => {
            let request = tonic::Request::new(ShutdownRequest {});
            let response = client.shutdown(request).await?;
            format!("{response:?}")
        }
        "get-dmesg" => {
            let request = tonic::Request::new(GetDmesgRequest {});
            let response = client.get_dmesg(request).await?;
            format!("{response:?}")
        }

        "snapshot" => {
            let vm = state.write_vms()?.remove(&vm_id).unwrap();
            let snapshot_request = VmSnapshotRequest {
                snapshot_type: SnapshotType::Full,
                snapshot_path: vm
                    .vm_state
                    .data_directory
                    .join("snapshot_path")
                    .to_string_lossy()
                    .into(),
                mem_file_path: vm
                    .vm_state
                    .data_directory
                    .join("mem_file")
                    .to_string_lossy()
                    .into(),
                version: "1.1.0".into(),
            };

            let vm = vm
                .pause_vm()
                .await?
                .snapshot_vm(&snapshot_request)
                .await?
                .resume_vm()
                .await?;

            state.write_vms()?.insert(vm_id.clone(), vm);

            format!("Successfully took snapshot of vm {vm_id}. Stored snapshot file at {} and memory file at {}", snapshot_request.snapshot_path, snapshot_request.mem_file_path)
        }
        command => Err(anyhow::anyhow!("Unknown command: {command}"))?,
    };

    if &src_ns != &target_ns {
        src_ns.enter()?;
    }

    Ok(response)
}

async fn list_vms(State(state): State<AppState>) -> Result<String, AppError> {
    let vms = state.read_vms()?;

    let ids: Vec<String> = vms.keys().cloned().collect();

    Ok(format!("{ids:?}"))
}
async fn resume_vm(
    State(state): State<AppState>,
    Json(load_snapshot_request): Json<LoadSnapshotRequest>,
) -> Result<String, AppError> {
    let config = &state.config;
    let vm_id = {
        let vms = state.read_vms()?;
        let mut id = Uuid::new_v4();
        while vms.contains_key(&id.to_string()) {
            id = Uuid::new_v4();
        }

        id
    };

    let vm = VirtualMachine::new(&config.firecracker_path, vm_id.clone())
        .await?
        .with_logger()
        .await?
        .load_snapshot(&config.host_network_interface, load_snapshot_request)
        .await?
        .resume_vm()
        .await?;

    let mut vms = state.write_vms()?;
    vms.insert(vm_id.to_string(), vm);

    Ok(format!("Successfully resume vm with id {vm_id}"))
}

async fn create_vm(State(state): State<AppState>) -> Result<String, AppError> {
    let config = &state.config;
    let vm_id = {
        let vms = state.read_vms()?;
        let mut id = Uuid::new_v4();
        while vms.contains_key(&id.to_string()) {
            id = Uuid::new_v4();
        }

        id
    };

    let vm = VirtualMachine::new(&config.firecracker_path, vm_id.clone())
        .await?
        .with_logger()
        .await?
        .setup_boot_source(VmBootSource {
            kernel_image_path: config.kernel_image_path.clone(),
            boot_args: config.boot_args.clone(),
        })
        .await?
        .add_network_interface(&config.host_network_interface, "172.16.0.2", BRIDGE_IP)
        .await?;

    let rootfs_path = vm.vm_state.data_directory.join("rootfs");
    std::fs::copy(&config.root_fs_path, &rootfs_path)?;

    let vm = vm
        .with_drive(VmDrive {
            drive_id: "rootfs".into(),
            path_on_host: rootfs_path.to_string_lossy().into(),
            is_root_device: true,
            is_read_only: false,
        })
        .await?
        .start()
        .await?;

    let mut vms = state.write_vms()?;
    vms.insert(vm_id.to_string(), vm);

    Ok(format!("Successfully spawned vm with id {vm_id}"))
}

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
