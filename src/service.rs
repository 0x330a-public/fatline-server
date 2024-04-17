use fatline_rs::HubService;

pub struct Service {
    hub_client: HubService
}

impl Service {
    pub async fn new(server_url: String) -> Self {
        let client = HubService::connect(server_url).await.expect("Couldn't build hub client");
        Self {
            hub_client: client
        }
    }
}
