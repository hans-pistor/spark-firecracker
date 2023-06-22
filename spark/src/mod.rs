use crate::api::{vm_actions_server::VmActions, PingRequest, PingResponse};

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
}
