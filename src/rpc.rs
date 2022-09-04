use chrono::{Duration, Utc};
use cln_rpc::model::*;
use cln_rpc::primitives::ShortChannelId;
use cln_rpc::ClnRpc;
use log::{debug, error};
use std::collections::HashMap;

pub async fn get_revenue_since(
    epoch_length: u32,
    short_channel_id: ShortChannelId,
    client: &mut ClnRpc,
) -> u64 {
    let last_updated = (Utc::now() + Duration::hours(epoch_length.into())).timestamp() as f64;
    let mut revenue = 0;
    if let Response::ListForwards(forwards) = client
        .call(Request::ListForwards(ListforwardsRequest {
            status: Some(ListforwardsStatus::SETTLED),
            in_channel: None,
            out_channel: Some(short_channel_id),
        }))
        .await
        .unwrap()
    {
        for payment in forwards.forwards {
            if payment.received_time > last_updated {
                revenue += payment.fee_msat.unwrap().msat();
            }
        }
    }
    revenue
}
pub async fn get_current_peers(client: &mut ClnRpc) -> Vec<ListpeersPeers> {
    if let Response::ListPeers(peers) = client
        .call(Request::ListPeers(ListpeersRequest {
            id: None,
            level: None,
        }))
        .await
        .unwrap()
    {
        peers.peers
    } else {
        vec![]
    }
}
pub async fn get_current_fees(client: &mut ClnRpc) -> HashMap<String, u32> {
    let mut fees = HashMap::new();
    for peer in get_current_peers(client).await {
        if !peer.connected {
            continue;
        }
        for channel in peer.channels {
            if let ListpeersPeersChannelsState::CHANNELD_NORMAL = channel.state {
                fees.insert(
                    channel.short_channel_id.unwrap().to_string(),
                    channel.fee_proportional_millionths.unwrap(),
                );
            }
        }
    }
    fees
}
pub async fn set_channel_fee(client: &mut ClnRpc, channel: &String, fee: u32) {
    match client
        .call(Request::SetChannel(SetChannelRequest {
            id: channel.clone(),
            feebase: None,
            feeppm: Some(fee),
            htlcmin_masat: None,
            htlcmax_msat: None,
        }))
        .await
    {
        Ok(response) => {
            if let Response::SetChannel(_channels) = response {
                debug!("Set fee {} msats for {}", fee, channel);
            }
        }
        Err(e) => error!("Couldn't set new fee for channel {}: {:?}", channel, e),
    }
}
