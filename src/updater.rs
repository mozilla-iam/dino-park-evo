use crate::groups::update_groups;
use cis_client::getby::GetBy;
use cis_client::AsyncCisClientTrait;
use failure::format_err;
use failure::Error;
use futures::future::Either;
use futures::prelude::*;
use futures::stream::iter_ok;
use futures::Future;
use futures::IntoFuture;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

#[derive(Deserialize, Clone, Debug)]
pub struct GroupUpdate {
    user_id: String,
    groups: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum UpdateMessage {
    Update(GroupUpdate),
    Bulk(Vec<GroupUpdate>),
    Stop,
}

pub trait UpdaterClient {
    fn update(&self, message: UpdateMessage);
    fn stop(&self);
}

pub trait Updater<U: UpdaterClient> {
    fn client(&self) -> U;
}

#[derive(Clone)]
pub struct InternalUpdaterClient {
    sender: Arc<RwLock<Sender<UpdateMessage>>>,
}

impl UpdaterClient for InternalUpdaterClient {
    fn update(&self, message: UpdateMessage) {
        if let Err(e) = (*self.sender).write().map(|mut w| w.try_send(message)) {
            warn!("unable to send internally: {}", e);
        }
    }
    fn stop(&self) {
        if let Err(e) = (*self.sender)
            .write()
            .map(|mut w| w.try_send(UpdateMessage::Stop))
        {
            warn!("unable to internally send stop message: {}", e);
        }
    }
}

pub struct InternalUpdater<T: AsyncCisClientTrait + Clone + Sync + Send> {
    cis_client: T,
    sender: Sender<UpdateMessage>,
    receiver: Receiver<UpdateMessage>,
}

impl<T: AsyncCisClientTrait + Clone + Sync + Send + 'static> InternalUpdater<T> {
    pub fn new(cis_client: T) -> Self {
        let (sender, receiver) = channel::<UpdateMessage>(100);
        InternalUpdater {
            cis_client,
            sender,
            receiver,
        }
    }

    pub fn run(self) -> impl Future<Item = (), Error = ()> {
        info!("start processing msgs");
        let cis_client = self.cis_client.clone();
        self.receiver
            .map_err(|e| format_err!("{}", e))
            .and_then(move |msg| {
                if let UpdateMessage::Stop = msg {}
                match msg {
                    UpdateMessage::Update(u) => Either::A(Either::A(update(&cis_client, u))),
                    UpdateMessage::Bulk(u) => Either::A(Either::B(update_batch(&cis_client, u))),
                    UpdateMessage::Stop => Either::B(Ok::<bool, Error>(false).into_future()),
                }
            })
            .map_err(|e| format_err!("{}", e))
            .map_err(|_| ())
            .fold(0, |x, b| {
                if b {
                    info!("processed {} messages so far", x + 1);
                    return futures::future::ok(x + 1);
                }
                info!("stopping");
                futures::future::err(())
            })
            .map(|_| {})
    }
}

impl<T: AsyncCisClientTrait + Clone + Sync + Send> Updater<InternalUpdaterClient>
    for InternalUpdater<T>
{
    fn client(&self) -> InternalUpdaterClient {
        InternalUpdaterClient {
            sender: Arc::new(RwLock::new(self.sender.clone())),
        }
    }
}

pub fn update<T: AsyncCisClientTrait + Clone + Sync + Send>(
    cis_client: &T,
    group_update: GroupUpdate,
) -> impl Future<Item = bool, Error = Error> {
    let user_id = group_update.user_id.clone();
    let user_id_success = group_update.user_id.clone();
    let user_id_fail = group_update.user_id.clone();
    let user_id_send = group_update.user_id;
    let cis_sign_client = cis_client.clone();
    let cis_update_client = cis_client.clone();
    let groups = group_update.groups;
    cis_client
        .get_user_by(&user_id, &GetBy::UserId, None)
        .and_then(move |profile| {
            info!("updating groups for: {}", &user_id);
            update_groups(profile, groups, cis_sign_client.get_secret_store())
                .map_err(|e| {
                    warn!("error updating groups: {}", e);
                    e
                })
                .into_future()
        })
        .and_then(move |updated_profile| {
            debug!("sending groups");
            cis_update_client.update_user(&user_id_send, updated_profile)
        })
        .map_err(move |e| {
            warn!("unable to publish update for {}: {}", user_id_fail, e);
        })
        .map(move |_| {
            info!("updated {}", user_id_success);
        })
        .then(|_| Ok(true).into_future())
}

pub fn update_batch<T: AsyncCisClientTrait + Clone + Sync + Send>(
    cis_client: &T,
    updates: Vec<GroupUpdate>,
) -> impl Future<Item = bool, Error = Error> {
    info!("bulk publishing: start");
    let len = updates.len();
    let client = cis_client.clone();
    iter_ok(updates)
        .and_then(move |u| update(&client, u))
        .fold(0, move |x, b| {
            if b {
                info!("updated {} profiles so far", x + 1);
                return futures::future::ok(x + 1);
            }
            info!("stopping");
            futures::future::err(format_err!("stopping bulk after {}/{} profiles", x, len))
        })
        .map(move |x| {
            info!("bulk updated {}/{} profiles", x, len);
        })
        .then(|_| Ok(true).into_future())
}
