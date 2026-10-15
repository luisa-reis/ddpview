//! Telling the network a WLED device is here, as WLED does: an mDNS
//! (Bonjour) service `_wled._tcp` under the device's host name, with its
//! card address in the record, which is how apps that look for WLED
//! devices find them.

use std::net::IpAddr;
use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::wled::Mac;

const SERVICE: &str = "_wled._tcp.local.";
/// How long going away waits for the goodbye to be sent.
const GOODBYE: Duration = Duration::from_millis(300);

/// The announcement, for as long as it is kept. Dropping it says goodbye.
pub struct Announcement {
    daemon: ServiceDaemon,
    fullname: String,
    host: String,
}

impl Announcement {
    /// Announces the device's HTTP API at `ip` (every address this
    /// computer has, when unspecified) on `port`.
    pub fn start(mac: Mac, ip: IpAddr, port: u16) -> Result<Self, String> {
        let host = mac.host();
        let hostname = format!("{host}.local.");
        let properties = [("mac", mac.to_string())];
        let service = if ip.is_unspecified() {
            ServiceInfo::new(SERVICE, &host, &hostname, (), port, &properties[..])
                .map(ServiceInfo::enable_addr_auto)
        } else {
            ServiceInfo::new(SERVICE, &host, &hostname, ip, port, &properties[..])
        }
        .map_err(|e| e.to_string())?;
        let fullname = service.get_fullname().to_string();
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        daemon.register(service).map_err(|e| e.to_string())?;
        Ok(Self {
            daemon,
            fullname,
            host: format!("{host}.local"),
        })
    }

    /// The name the device can be reached by, e.g. `ddpview-aabbcc.local`.
    pub fn host(&self) -> &str {
        &self.host
    }
}

impl Drop for Announcement {
    fn drop(&mut self) {
        // So that those who saw it forget it now, not when it times out.
        if let Ok(done) = self.daemon.unregister(&self.fullname) {
            let _ = done.recv_timeout(GOODBYE);
        }
        let _ = self.daemon.shutdown();
    }
}
