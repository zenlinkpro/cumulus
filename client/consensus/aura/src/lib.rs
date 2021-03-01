// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! The relay-chain provided consensus algoritm for parachains.
//!
//! This is the simplest consensus algorithm you can use when developing a parachain. It is a
//! permission-less consensus algorithm that doesn't require any staking or similar to join as a
//! collator. In this algorithm the consensus is provided by the relay-chain. This works in the
//! following way.
//!
//! 1. Each node that sees itself as a collator is free to build a parachain candidate.
//!
//! 2. This parachain candidate is send to the parachain validators that are part of the relay chain.
//!
//! 3. The parachain validators validate at most X different parachain candidates, where X is the
//! total number of parachain validators.
//!
//! 4. The parachain candidate that is backed by the most validators is choosen by the relay-chain
//! block producer to be added as backed candidate on chain.
//!
//! 5. After the parachain candidate got backed and included, all collators start at 1.

use codec::{Decode, Encode};
use cumulus_client_consensus_common::{ParachainCandidate, ParachainConsensus};
use cumulus_primitives_core::{
	relay_chain::v1::{Block as PBlock, Hash as PHash, ParachainHost},
	ParaId, PersistedValidationData,
};
use cumulus_primitives_parachain_inherent::ParachainInherentData;
use parking_lot::Mutex;
use polkadot_service::ClientHandle;
use sc_client_api::{backend::AuxStore, Backend, BlockOf};
use sc_consensus_slots::{BackoffAuthoringBlocksStrategy, SlotInfo};
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_blockchain::{HeaderBackend, ProvideCache};
use sp_consensus::{
	BlockImport, BlockImportParams, BlockOrigin, EnableProofRecording, Environment,
	ForkChoiceStrategy, ProofRecording, Proposal, Proposer, SyncOracle,
};
use sp_consensus_aura::AuraApi;
use sp_core::crypto::Pair;
use sp_inherents::{CreateInherentDataProviders, InherentData, InherentDataProvider};
use sp_keystore::SyncCryptoStorePtr;
use sp_runtime::traits::{Block as BlockT, HashFor, Header as HeaderT, Member, NumberFor};
use std::{convert::TryFrom, hash::Hash, marker::PhantomData, sync::Arc};

pub use sc_consensus_aura::import_queue;

const LOG_TARGET: &str = "aura::cumulus";

/// The implementation of the AURA consensus for parachains.
pub struct AuraConsensus<B, RClient, RBackend, CIDP> {
	inherent_data_providers: Arc<CIDP>,
	relay_chain_client: Arc<RClient>,
	relay_chain_backend: Arc<RBackend>,
	aura_worker: Arc<
		Mutex<
			dyn sc_consensus_slots::SlotWorker<B, <EnableProofRecording as ProofRecording>::Proof>
				+ Send
				+ 'static,
		>,
	>,
}

impl<B, RClient, RBackend, CIDP> Clone for AuraConsensus<B, RClient, RBackend, CIDP> {
	fn clone(&self) -> Self {
		Self {
			inherent_data_providers: self.inherent_data_providers.clone(),
			relay_chain_backend: self.relay_chain_backend.clone(),
			relay_chain_client: self.relay_chain_client.clone(),
			aura_worker: self.aura_worker.clone(),
		}
	}
}

impl<B, RClient, RBackend, CIDP> AuraConsensus<B, RClient, RBackend, CIDP>
where
	B: BlockT,
	RClient: ProvideRuntimeApi<PBlock>,
	RClient::Api: ParachainHost<PBlock>,
	RBackend: Backend<PBlock>,
	CIDP: CreateInherentDataProviders<B, (PHash, PersistedValidationData)>,
{
	/// Create a new instance of AURA consensus.
	pub fn new<P, Client, BI, SO, PF, BS, Error>(
		para_client: Arc<Client>,
		block_import: BI,
		sync_oracle: SO,
		proposer_factory: PF,
		force_authoring: bool,
		backoff_authoring_blocks: Option<BS>,
		keystore: SyncCryptoStorePtr,
		inherent_data_providers: CIDP,
		polkadot_client: Arc<RClient>,
		polkadot_backend: Arc<RBackend>,
	) -> Self
	where
		Client: ProvideRuntimeApi<B>
			+ BlockOf
			+ ProvideCache<B>
			+ AuxStore
			+ HeaderBackend<B>
			+ Send
			+ Sync
			+ 'static,
		Client::Api: AuraApi<B, P::Public>,
		BI: BlockImport<B, Transaction = sp_api::TransactionFor<Client, B>> + Send + Sync + 'static,
		SO: SyncOracle + Send + Sync + Clone + 'static,
		BS: BackoffAuthoringBlocksStrategy<NumberFor<B>> + Send + 'static,
		PF: Environment<B, Error = Error> + Send + Sync + 'static,
		PF::Proposer: Proposer<
			B,
			Error = Error,
			Transaction = sp_api::TransactionFor<Client, B>,
			ProofRecording = EnableProofRecording,
			Proof = <EnableProofRecording as ProofRecording>::Proof,
		>,
		Error: std::error::Error + Send + From<sp_consensus::Error> + 'static,
		P: Pair + Send + Sync,
		P::Public: AppPublic + Hash + Member + Encode + Decode,
		P::Signature: TryFrom<Vec<u8>> + Hash + Member + Encode + Decode,
	{
		let worker = sc_consensus_aura::build_aura_worker::<_, _, _, _, P, _, _, _>(
			para_client,
			block_import,
			proposer_factory,
			sync_oracle,
			force_authoring,
			backoff_authoring_blocks,
			keystore,
		);

		Self {
			inherent_data_providers: Arc::new(inherent_data_providers),
			relay_chain_backend: polkadot_backend,
			relay_chain_client: polkadot_client,
			aura_worker: Arc::new(Mutex::new(worker)),
		}
	}

	/// Get the inherent data with validation function parameters injected
	async fn inherent_data(
		&self,
		parent: B::Hash,
		validation_data: &PersistedValidationData,
		relay_parent: PHash,
	) -> Option<InherentData> {
		let inherent_data_providers = self
			.inherent_data_providers
			.create_inherent_data_providers(parent, (relay_parent, validation_data.clone()))
			.await
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Failed to create inherent data providers.",
				)
			})
			.ok()?;

		inherent_data_providers
			.create_inherent_data()
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Failed to create inherent data.",
				)
			})
			.ok()
	}
}

#[async_trait::async_trait]
impl<B, RClient, RBackend, CIDP> ParachainConsensus<B> for AuraConsensus<B, RClient, RBackend, CIDP>
where
	B: BlockT,
	RClient: ProvideRuntimeApi<PBlock> + Send + Sync,
	RClient::Api: ParachainHost<PBlock>,
	RBackend: Backend<PBlock>,
	CIDP: CreateInherentDataProviders<B, (PHash, PersistedValidationData)> + Send + Sync,
{
	async fn produce_candidate(
		&mut self,
		parent: &B::Header,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
	) -> Option<ParachainCandidate<B>> {
		let timestamp = std::time::SystemTime::now()
			.duration_since(std::time::SystemTime::UNIX_EPOCH)
			.unwrap();

		let info = SlotInfo {
			slot: ((timestamp.as_millis() / 12000) as u64).into(),
			duration: 12000,
			inherent_data: self
				.inherent_data(parent.hash(), validation_data, relay_parent)
				.await?,
			chain_head: parent.clone(),
			timestamp,
			ends_at: std::time::Instant::now() + std::time::Duration::from_millis(500),
		};

		let future = self.aura_worker.lock().on_slot(info);
		let res = future.await?;

		Some(ParachainCandidate {
			block: res.block,
			proof: res.storage_proof,
		})
	}
}

/// Paramaters of [`build_aura_consensus`].
pub struct BuildAuraConsensusParams<PF, BI, RBackend, CIDP, Client, BS, SO> {
	pub proposer_factory: PF,
	pub inherent_data_providers: CIDP,
	pub block_import: BI,
	pub relay_chain_client: polkadot_service::Client,
	pub relay_chain_backend: Arc<RBackend>,
	pub para_client: Arc<Client>,
	pub backoff_authoring_blocks: Option<BS>,
	pub sync_oracle: SO,
	pub keystore: SyncCryptoStorePtr,
	pub force_authoring: bool,
}

/// Build the [`AuraConsensus`].
///
/// Returns a boxed [`ParachainConsensus`].
pub fn build_aura_consensus<P, Block, PF, BI, RBackend, CIDP, Client, SO, BS, Error>(
	BuildAuraConsensusParams {
		proposer_factory,
		inherent_data_providers,
		block_import,
		relay_chain_client,
		relay_chain_backend,
		para_client,
		backoff_authoring_blocks,
		sync_oracle,
		keystore,
		force_authoring,
	}: BuildAuraConsensusParams<PF, BI, RBackend, CIDP, Client, BS, SO>,
) -> Box<dyn ParachainConsensus<Block>>
where
	Block: BlockT,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<RBackend, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
	RBackend: Backend<PBlock> + 'static,
	CIDP: CreateInherentDataProviders<Block, (PHash, PersistedValidationData)>
		+ Send
		+ Sync
		+ 'static,
	Client: ProvideRuntimeApi<Block>
		+ BlockOf
		+ ProvideCache<Block>
		+ AuxStore
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: AuraApi<Block, P::Public>,
	BI: BlockImport<Block, Transaction = sp_api::TransactionFor<Client, Block>>
		+ Send
		+ Sync
		+ 'static,
	SO: SyncOracle + Send + Sync + Clone + 'static,
	BS: BackoffAuthoringBlocksStrategy<NumberFor<Block>> + Send + 'static,
	PF: Environment<Block, Error = Error> + Send + Sync + 'static,
	PF::Proposer: Proposer<
		Block,
		Error = Error,
		Transaction = sp_api::TransactionFor<Client, Block>,
		ProofRecording = EnableProofRecording,
		Proof = <EnableProofRecording as ProofRecording>::Proof,
	>,
	Error: std::error::Error + Send + From<sp_consensus::Error> + 'static,
	P: Pair + Send + Sync,
	P::Public: AppPublic + Hash + Member + Encode + Decode,
	P::Signature: TryFrom<Vec<u8>> + Hash + Member + Encode + Decode,
{
	AuraConsensusBuilder::<P, _, _, _, _, _, _, _, _, _>::new(
		proposer_factory,
		block_import,
		inherent_data_providers,
		relay_chain_client,
		relay_chain_backend,
		para_client,
		backoff_authoring_blocks,
		sync_oracle,
		force_authoring,
		keystore,
	)
	.build()
}

/// Aura consensus builder.
///
/// Builds a [`AuraConsensus`] for a parachain. As this requires
/// a concrete relay chain client instance, the builder takes a [`polkadot_service::Client`]
/// that wraps this concrete instanace. By using [`polkadot_service::ExecuteWithClient`]
/// the builder gets access to this concrete instance.
struct AuraConsensusBuilder<P, Block, PF, BI, RBackend, CIDP, Client, SO, BS, Error> {
	_phantom: PhantomData<(Block, Error, P)>,
	proposer_factory: PF,
	inherent_data_providers: CIDP,
	block_import: BI,
	relay_chain_backend: Arc<RBackend>,
	relay_chain_client: polkadot_service::Client,
	para_client: Arc<Client>,
	backoff_authoring_blocks: Option<BS>,
	sync_oracle: SO,
	force_authoring: bool,
	keystore: SyncCryptoStorePtr,
}

impl<Block, PF, BI, RBackend, CIDP, Client, SO, BS, P, Error>
	AuraConsensusBuilder<P, Block, PF, BI, RBackend, CIDP, Client, SO, BS, Error>
where
	Block: BlockT,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<RBackend, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
	RBackend: Backend<PBlock> + 'static,
	CIDP: CreateInherentDataProviders<Block, (PHash, PersistedValidationData)>
		+ Send
		+ Sync
		+ 'static,
	Client: ProvideRuntimeApi<Block>
		+ BlockOf
		+ ProvideCache<Block>
		+ AuxStore
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: AuraApi<Block, P::Public>,
	BI: BlockImport<Block, Transaction = sp_api::TransactionFor<Client, Block>>
		+ Send
		+ Sync
		+ 'static,
	SO: SyncOracle + Send + Sync + Clone + 'static,
	BS: BackoffAuthoringBlocksStrategy<NumberFor<Block>> + Send + 'static,
	PF: Environment<Block, Error = Error> + Send + Sync + 'static,
	PF::Proposer: Proposer<
		Block,
		Error = Error,
		Transaction = sp_api::TransactionFor<Client, Block>,
		ProofRecording = EnableProofRecording,
		Proof = <EnableProofRecording as ProofRecording>::Proof,
	>,
	Error: std::error::Error + Send + From<sp_consensus::Error> + 'static,
	P: Pair + Send + Sync,
	P::Public: AppPublic + Hash + Member + Encode + Decode,
	P::Signature: TryFrom<Vec<u8>> + Hash + Member + Encode + Decode,
{
	/// Create a new instance of the builder.
	fn new(
		proposer_factory: PF,
		block_import: BI,
		inherent_data_providers: CIDP,
		relay_chain_client: polkadot_service::Client,
		relay_chain_backend: Arc<RBackend>,
		para_client: Arc<Client>,
		backoff_authoring_blocks: Option<BS>,
		sync_oracle: SO,
		force_authoring: bool,
		keystore: SyncCryptoStorePtr,
	) -> Self {
		Self {
			_phantom: PhantomData,
			proposer_factory,
			block_import,
			inherent_data_providers,
			relay_chain_backend,
			relay_chain_client,
			para_client,
			backoff_authoring_blocks,
			sync_oracle,
			force_authoring,
			keystore,
		}
	}

	/// Build the relay chain consensus.
	fn build(self) -> Box<dyn ParachainConsensus<Block>> {
		self.relay_chain_client.clone().execute_with(self)
	}
}

impl<Block, PF, BI, RBackend, CIDP, Client, SO, BS, P, Error> polkadot_service::ExecuteWithClient
	for AuraConsensusBuilder<P, Block, PF, BI, RBackend, CIDP, Client, SO, BS, Error>
where
	Block: BlockT,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<RBackend, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
	RBackend: Backend<PBlock> + 'static,
	CIDP: CreateInherentDataProviders<Block, (PHash, PersistedValidationData)>
		+ Send
		+ Sync
		+ 'static,
	Client: ProvideRuntimeApi<Block>
		+ BlockOf
		+ ProvideCache<Block>
		+ AuxStore
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: AuraApi<Block, P::Public>,
	BI: BlockImport<Block, Transaction = sp_api::TransactionFor<Client, Block>>
		+ Send
		+ Sync
		+ 'static,
	SO: SyncOracle + Send + Sync + Clone + 'static,
	BS: BackoffAuthoringBlocksStrategy<NumberFor<Block>> + Send + 'static,
	PF: Environment<Block, Error = Error> + Send + Sync + 'static,
	PF::Proposer: Proposer<
		Block,
		Error = Error,
		Transaction = sp_api::TransactionFor<Client, Block>,
		ProofRecording = EnableProofRecording,
		Proof = <EnableProofRecording as ProofRecording>::Proof,
	>,
	Error: std::error::Error + Send + From<sp_consensus::Error> + 'static,
	P: Pair + Send + Sync,
	P::Public: AppPublic + Hash + Member + Encode + Decode,
	P::Signature: TryFrom<Vec<u8>> + Hash + Member + Encode + Decode,
{
	type Output = Box<dyn ParachainConsensus<Block>>;

	fn execute_with_client<PClient, Api, PBackend>(self, client: Arc<PClient>) -> Self::Output
	where
		<Api as sp_api::ApiExt<PBlock>>::StateBackend: sp_api::StateBackend<HashFor<PBlock>>,
		PBackend: Backend<PBlock>,
		PBackend::State: sp_api::StateBackend<sp_runtime::traits::BlakeTwo256>,
		Api: polkadot_service::RuntimeApiCollection<StateBackend = PBackend::State>,
		PClient: polkadot_service::AbstractClient<PBlock, PBackend, Api = Api> + 'static,
	{
		Box::new(AuraConsensus::new::<P, _, _, _, _, _, _>(
			self.para_client,
			self.block_import,
			self.sync_oracle,
			self.proposer_factory,
			self.force_authoring,
			self.backoff_authoring_blocks,
			self.keystore,
			self.inherent_data_providers,
			client.clone(),
			self.relay_chain_backend,
		))
	}
}