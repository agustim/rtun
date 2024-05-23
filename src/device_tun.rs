use tokio_tun::Tun;
use ipnet::Ipv4Net;
use std::net::Ipv4Addr;
use std::sync::Arc;
use log::{info, error};
use std::error::Error;

pub async fn create_tun(iface: &str, ipv4: &str, mtu: i32) -> Result<Arc<Tun>, Box<dyn Error>> {
    let net: Ipv4Net = ipv4.parse().map_err(|e| {
        error!("Failed to parse IPv4 address: {}", e);
        e
    })?;

    let tun = Tun::builder()
        .name(iface)
        .tap(false)
        .packet_info(false)
        .mtu(mtu)
        .up()
        .address(net.addr())
        .broadcast(Ipv4Addr::BROADCAST)
        .netmask(net.netmask())
        .try_build()
        .map_err(|e| {
            error!("Failed to create Tun interface: {}", e);
            e
        })?;
    info!("Tun interface created: {:?}, with IP {}", tun.name(), ipv4);
    Ok(Arc::new(tun))
}