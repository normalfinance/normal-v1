pub mod pyth_program {
	use solana_program::declare_id;
	#[cfg(feature = "mainnet-beta")]
	declare_id!("FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH");
	#[cfg(not(feature = "mainnet-beta"))]
	declare_id!("gSbePebfvPy7tRqimPoVecS2UsBvYv46ynrzWocc92s");
}

pub mod normal_oracle_receiver_program {
	use solana_program::declare_id;
	declare_id!("G6EoTTTgpkNBtVXo96EQp2m6uwwVh2Kt6YidjkmQqoha");
}

pub mod bonk_oracle {
	use solana_program::declare_id;
	#[cfg(feature = "mainnet-beta")]
	declare_id!("8ihFLu5FimgTQ1Unh4dVyEHUGodJ5gJQCrQf4KUVB9bN");
	#[cfg(not(feature = "mainnet-beta"))]
	declare_id!("6bquU99ktV1VRiHDr8gMhDFt3kMfhCQo5nfNrg2Urvsn");
}

pub mod bonk_pull_oracle {
	use solana_program::declare_id;
	declare_id!("GojbSnJuPdKDT1ZuHuAM5t9oz6bxTo1xhUKpTua2F72p");
}

pub mod pepe_oracle {
	use solana_program::declare_id;
	#[cfg(feature = "mainnet-beta")]
	declare_id!("FSfxunDmjjbDV2QxpyxFCAPKmYJHSLnLuvQXDLkMzLBm");
	#[cfg(not(feature = "mainnet-beta"))]
	declare_id!("Gz9RfgDeAFSsH7BHDGyNTgCik74rjNwsodJpsCizzmkj");
}

pub mod pepe_pull_oracle {
	use solana_program::declare_id;
	declare_id!("CLxofhtzvLiErpn25wvUzpZXEqBhuZ6WMEckEraxyuGt");
}

pub mod usdc_oracle {
	use solana_program::declare_id;
	#[cfg(feature = "mainnet-beta")]
	declare_id!("Gnt27xtC473ZT2Mw5u8wZ68Z3gULkSTb5DuxJy7eJotD");
	#[cfg(not(feature = "mainnet-beta"))]
	declare_id!("5SSkXsEKQepHHAewytPVwdej4epN1nxgLVM84L4KXgy7");
}

pub mod usdc_pull_oracle {
	use solana_program::declare_id;
	declare_id!("En8hkHLkRe9d9DraYmBTrus518BvmVH448YcvmrFM6Ce");
}

pub mod usdt_oracle {
	use solana_program::declare_id;
	declare_id!("3vxLXJqLqF3JG5TCbYycbKWRBbCJQLxQmBGCkyqEEefL");
}

pub mod usdt_pull_oracle {
	use solana_program::declare_id;
	declare_id!("BekJ3P5G3iFeC97sXHuKnUHofCFj9Sbo7uyF2fkKwvit");
}

pub mod admin_hot_wallet {
	use solana_program::declare_id;
	declare_id!("5hMjmxexWu954pX9gB9jkHxMqdjpxArQS2XdvkaevRax");
}
