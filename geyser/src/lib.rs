use {
    agave_geyser_plugin_interface::geyser_plugin_interface::{
        GeyserPlugin, GeyserPluginError, ReplicaAccountInfoVersions, ReplicaTransactionInfoVersions,
        Result as PluginResult,
    },
    serde::Deserialize,
    solana_clock::Slot,
    std::{
        env,
        fs::File,
        io::Read,
        sync::{Arc, RwLock},
    },
};

const PROGRAM_ID_ENV: &str = "DOPPLER_PROGRAM_ID";

/// `Oracle<PriceFeed>` account size: 8-byte sequence + 8-byte price.
const ORACLE_ACCOUNT_LEN: usize = 16;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PluginConfig {
    /// Base58 program id override. Falls back to `DOPPLER_PROGRAM_ID` env var.
    program_id: Option<String>,
    /// Log transaction notifications that touch tracked oracle accounts.
    log_transactions: bool,
}

#[derive(Debug)]
struct PluginState {
    program_id: [u8; 32],
    log_transactions: bool,
}

impl Default for PluginState {
    fn default() -> Self {
        Self {
            // fastRQJt3nLdY3QA7n8eZ8ETEVefy56ryfUGVkfZokm
            program_id: [
                0x09, 0xe2, 0x60, 0x40, 0xff, 0x10, 0xec, 0xcf, 0xc1, 0x6a, 0xf6, 0x16, 0x9a, 0x68, 0x04, 0x78,
                0x15, 0x14, 0x33, 0x02, 0xac, 0x6e, 0x98, 0x5f, 0x70, 0x85, 0x53, 0xe1, 0x0a, 0xb6, 0xf9, 0x22,
            ],
            log_transactions: true,
        }
    }
}

#[derive(Debug)]
pub struct DopplerGeyserPlugin {
    state: Arc<RwLock<PluginState>>,
}

impl Default for DopplerGeyserPlugin {
    fn default() -> Self {
        Self {
            state: Arc::new(RwLock::new(PluginState::default())),
        }
    }
}

impl DopplerGeyserPlugin {
    fn decode_pubkey(value: &str, field: &str) -> PluginResult<[u8; 32]> {
        let mut out = [0u8; 32];
        bs58::decode(value)
            .onto(&mut out)
            .map_err(|err| GeyserPluginError::ConfigFileReadError {
                msg: format!("invalid {field} pubkey `{value}`: {err}"),
            })?;
        Ok(out)
    }

    fn resolve_program_id(config: &PluginConfig) -> PluginResult<[u8; 32]> {
        if let Some(value) = &config.program_id {
            return Self::decode_pubkey(value, "program_id");
        }

        let value = env::var(PROGRAM_ID_ENV).map_err(|_| GeyserPluginError::ConfigFileReadError {
            msg: format!("set {PROGRAM_ID_ENV} or provide program_id in the geyser config"),
        })?;
        Self::decode_pubkey(&value, PROGRAM_ID_ENV)
    }

    fn load_config(path: &str) -> PluginResult<PluginConfig> {
        let mut file = File::open(path).map_err(GeyserPluginError::ConfigFileOpenError)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(GeyserPluginError::ConfigFileOpenError)?;
        serde_json::from_str(&contents).map_err(|err| GeyserPluginError::ConfigFileReadError {
            msg: format!("failed to parse geyser config `{path}`: {err}"),
        })
    }

    fn tracked_oracle(&self, owner: &[u8], data: &[u8]) -> bool {
        owner == self.state.read().expect("lock poisoned").program_id.as_slice()
            && data.len() == ORACLE_ACCOUNT_LEN
    }

    fn log_oracle_update(
        &self,
        pubkey: &[u8],
        data: &[u8],
        slot: Slot,
        write_version: u64,
        is_startup: bool,
    ) {
        let sequence = u64::from_le_bytes(data[..8].try_into().expect("sequence bytes"));
        let price = u64::from_le_bytes(data[8..16].try_into().expect("price bytes"));

        log::info!(
            target: "doppler_geyser",
            "oracle update slot={slot} write_version={write_version} startup={is_startup} \
             pubkey={} sequence={sequence} price={price}",
            bs58::encode(pubkey).into_string(),
        );
    }
}

impl GeyserPlugin for DopplerGeyserPlugin {
    fn setup_logger(&self, logger: &'static dyn log::Log, level: log::LevelFilter) -> PluginResult<()> {
        log::set_max_level(level);
        log::set_logger(logger).map_err(|err| GeyserPluginError::Custom(Box::new(err)))?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "doppler-geyser"
    }

    fn on_load(&mut self, config_file: &str, _is_reload: bool) -> PluginResult<()> {
        let config = Self::load_config(config_file)?;
        let program_id = Self::resolve_program_id(&config)?;

        let mut state = self.state.write().expect("lock poisoned");
        state.program_id = program_id;
        state.log_transactions = config.log_transactions;

        log::info!(
            target: "doppler_geyser",
            "loaded config from {config_file}: program_id={} log_transactions={}",
            bs58::encode(program_id).into_string(),
            state.log_transactions,
        );

        Ok(())
    }

    fn update_account(
        &self,
        account: ReplicaAccountInfoVersions,
        slot: Slot,
        is_startup: bool,
    ) -> PluginResult<()> {
        let info = match account {
            ReplicaAccountInfoVersions::V0_0_3(info) => info,
            ReplicaAccountInfoVersions::V0_0_2(info) => {
                if !self.tracked_oracle(info.owner, info.data) {
                    return Ok(());
                }
                self.log_oracle_update(info.pubkey, info.data, slot, info.write_version, is_startup);
                return Ok(());
            }
            ReplicaAccountInfoVersions::V0_0_1(info) => {
                if !self.tracked_oracle(info.owner, info.data) {
                    return Ok(());
                }
                self.log_oracle_update(info.pubkey, info.data, slot, info.write_version, is_startup);
                return Ok(());
            }
        };

        if !self.tracked_oracle(info.owner, info.data) {
            return Ok(());
        }

        self.log_oracle_update(info.pubkey, info.data, slot, info.write_version, is_startup);
        Ok(())
    }

    fn notify_transaction(
        &self,
        transaction: ReplicaTransactionInfoVersions,
        slot: Slot,
    ) -> PluginResult<()> {
        if !self.state.read().expect("lock poisoned").log_transactions {
            return Ok(());
        }

        let (signature, is_vote) = match transaction {
            ReplicaTransactionInfoVersions::V0_0_1(info) => (info.signature, info.is_vote),
            ReplicaTransactionInfoVersions::V0_0_2(info) => (info.signature, info.is_vote),
        };

        if is_vote {
            return Ok(());
        }

        log::info!(
            target: "doppler_geyser",
            "transaction slot={slot} signature={signature}",
        );
        Ok(())
    }

    fn account_data_snapshot_notifications_enabled(&self) -> bool {
        false
    }

    fn transaction_notifications_enabled(&self) -> bool {
        self.state.read().expect("lock poisoned").log_transactions
    }
}

#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn _create_plugin() -> *mut dyn GeyserPlugin {
    let plugin: Box<dyn GeyserPlugin> = Box::<DopplerGeyserPlugin>::default();
    Box::into_raw(plugin)
}
