use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmBootSource {
    pub kernel_image_path: String,
    pub boot_args: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmDrive {
    pub drive_id: String,
    pub path_on_host: String,
    pub is_root_device: bool,
    pub is_read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmNetworkInterface {
    pub iface_id: String,
    pub guest_mac: String,
    pub host_dev_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmLogger {
    pub log_path: PathBuf,
    pub level: String,
    pub show_level: bool,
    pub show_log_origin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotType {
    Full
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSnapshotRequest {
    pub snapshot_type: SnapshotType,
    pub snapshot_path: String,
    pub mem_file_path: String,
    pub version: String
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackendType {
    File, Uffd
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBackend {
    pub backend_path: String,
    pub backend_type: BackendType
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadSnapshotRequest {
    pub snapshot_path: String,
    pub mem_backend: MemoryBackend,
    pub enable_diff_snapshots: bool,
    pub resume_vm: bool
}