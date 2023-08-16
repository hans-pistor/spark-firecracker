use crate::api::{
    vm_actions_server::VmActions, GetDmesgRequest, GetDmesgResponse, PingRequest, PingResponse,
    ShutdownRequest, ShutdownResponse,
};

pub mod net;
pub mod vm;

pub mod api {
    tonic::include_proto!("api");
}

#[derive(Debug, Default)]
pub struct VmService {}

#[tonic::async_trait]
impl VmActions for VmService {
    async fn ping(
        &self,
        _: tonic::Request<PingRequest>,
    ) -> Result<tonic::Response<PingResponse>, tonic::Status> {
        Ok(tonic::Response::new(PingResponse {}))
    }

    async fn shutdown(
        &self,
        _: tonic::Request<ShutdownRequest>,
    ) -> Result<tonic::Response<ShutdownResponse>, tonic::Status> {
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            std::process::Command::new("reboot")
                .output()
                .expect("Failed to reboot");
        });
        Ok(tonic::Response::new(ShutdownResponse {}))
    }

    async fn get_dmesg(
        &self,
        _: tonic::Request<GetDmesgRequest>,
    ) -> Result<tonic::Response<GetDmesgResponse>, tonic::Status> {
        let output = std::process::Command::new("dmesg").output().unwrap().stdout;
        Ok(tonic::Response::new(GetDmesgResponse {
            text: String::from_utf8_lossy(&output).into(),
        }))
    }
}
