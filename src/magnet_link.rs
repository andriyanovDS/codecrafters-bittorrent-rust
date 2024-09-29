use anyhow::Error;
use reqwest::Url;
use urlencoding;

const EXACT_TOPIC: &str = "xt";
const DISPLAY_NAME: &str = "dn";
const TRACKER_ADDRESS: &str = "tr";
const PEER_ADDRESS: &str = "x.pe";

pub struct MagnetLink {
    pub info_hash: InfoHash,
    pub display_name: Option<String>,
    pub tracker_address: Vec<Url>,
    pub peer_address: Vec<Url>,
}

pub struct InfoHash {
    pub urn: String,
    pub hash: String,
}

impl MagnetLink {
    pub fn parse(link: &str) -> Result<MagnetLink, Error> {
        let (prefix, params) = link.split_once("?").ok_or_else(|| {
            anyhow::Error::msg("Invalid magnet link")
        })?;
        assert!(prefix == "magnet:");
        let params = params
            .split("&")
            .filter_map(|param| param.split_once("="))
            .collect::<Vec<_>>();

        let info_hash = params.iter().find_map(|(name, value)| {
            if *name == EXACT_TOPIC {
                let mut values = value.split(":");
                assert!(values.next()? == "urn");
                Some(InfoHash {
                    urn: values.next()?.into(),
                    hash: values.next()?.into()
                })
            } else {
                None
            }
        });
        let Some(info_hash) = info_hash else {
            return Err(anyhow::Error::msg("info hash is required parameter in magnet link"));
        };
        let mut magnet_link = MagnetLink {
            info_hash,
            display_name: None,
            tracker_address: vec![],
            peer_address: vec![]
        };

        magnet_link.display_name = params.iter().find_map(|(name, value)| {
            if *name == DISPLAY_NAME {
                Some((*value).into())
            } else {
                None
            }
        });
        magnet_link.peer_address = params.iter().filter_map(|(name, value)| {
            if *name == PEER_ADDRESS {
                let decoded_url = urlencoding::decode(*&value).ok()?;
                reqwest::Url::parse(decoded_url.as_ref()).ok()
            } else {
                None
            }
        }).collect();
        magnet_link.tracker_address = params.iter().filter_map(|(name, value)| {
            if *name == TRACKER_ADDRESS {
                let decoded_url = urlencoding::decode(*&value).ok()?;
                reqwest::Url::parse(decoded_url.as_ref()).ok()
            } else {
                None
            }
        }).collect();

        Ok(magnet_link)
    }
}
