use mdns_sd::{ServiceDaemon, ServiceInfo};
use tracing::{info, warn};

const SERVICE_TYPE: &str = "_tephra._tcp.local.";

pub struct MdnsRegistration {
    daemon: ServiceDaemon,
    fullname: String,
}

impl MdnsRegistration {
    pub fn register(
        hostname: &str,
        port: u16,
        cpu_model: &str,
        core_count: usize,
    ) -> Option<Self> {
        let daemon = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to create mDNS daemon: {e}. Discovery will not be available.");
                return None;
            }
        };

        let instance_name = format!("tephra-{}", hostname);

        // Build TXT record properties
        let properties = [
            ("version", env!("CARGO_PKG_VERSION")),
            ("hostname", hostname),
        ];

        // Truncate CPU model to fit TXT record limits
        let cpu_short = if cpu_model.len() > 200 {
            &cpu_model[..200]
        } else {
            cpu_model
        };
        let cores_str = core_count.to_string();

        let mut props = Vec::new();
        for (k, v) in &properties {
            props.push((*k, *v));
        }
        props.push(("cpu", cpu_short));
        props.push(("cores", &cores_str));

        let service_info = match ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &format!("{}.local.", hostname),
            "",
            port,
            &props[..],
        ) {
            Ok(info) => info,
            Err(e) => {
                warn!("Failed to create mDNS service info: {e}");
                return None;
            }
        };

        let fullname = service_info.get_fullname().to_string();

        match daemon.register(service_info) {
            Ok(_) => {
                info!(
                    "Registered mDNS service: {} on port {}",
                    SERVICE_TYPE, port
                );
                Some(Self { daemon, fullname })
            }
            Err(e) => {
                warn!("Failed to register mDNS service: {e}");
                None
            }
        }
    }

    pub fn unregister(self) {
        if let Err(e) = self.daemon.unregister(&self.fullname) {
            warn!("Failed to unregister mDNS service: {e}");
        } else {
            info!("Unregistered mDNS service");
        }
        if let Err(e) = self.daemon.shutdown() {
            warn!("Failed to shut down mDNS daemon: {e}");
        }
    }
}
