use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use psyche_solana_authorizer::accounts::AuthorizationCreateAccounts;
use psyche_solana_authorizer::accounts::AuthorizationDelegatesAccounts;
use psyche_solana_authorizer::accounts::AuthorizationRevokeAccounts;
use psyche_solana_authorizer::find_authorization;
use psyche_solana_authorizer::instruction::AuthorizationCreate;
use psyche_solana_authorizer::instruction::AuthorizationDelegates;
use psyche_solana_authorizer::instruction::AuthorizationRevoke;
use psyche_solana_authorizer::logic::AuthorizationCreateParams;
use psyche_solana_authorizer::logic::AuthorizationDelegatesParams;
use psyche_solana_authorizer::logic::AuthorizationRevokeParams;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signature;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_endpoint::ToolboxEndpoint;
use solana_toolbox_endpoint::ToolboxEndpointError;

pub async fn process_authorizer_authorization_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    grantor: &Keypair,
    grantee: &Pubkey,
    scope: &[u8],
) -> Result<Signature, ToolboxEndpointError> {
    let authorization = find_authorization(&grantor.pubkey(), grantee, scope);

    let accounts = AuthorizationCreateAccounts {
        payer: payer.pubkey(),
        grantor: grantor.pubkey(),
        authorization,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: AuthorizationCreate {
            params: AuthorizationCreateParams {
                grantee: *grantee,
                scope: scope.to_vec(),
            },
        }
        .data(),
        program_id: psyche_solana_authorizer::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[grantor])
        .await
}

pub async fn process_authorizer_authorization_delegates(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    grantor: &Pubkey,
    grantee: &Keypair,
    scope: &[u8],
    delegates: &[Pubkey],
) -> Result<Signature, ToolboxEndpointError> {
    let authorization = find_authorization(grantor, &grantee.pubkey(), scope);

    let accounts = AuthorizationDelegatesAccounts {
        payer: payer.pubkey(),
        grantee: grantee.pubkey(),
        authorization,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: AuthorizationDelegates {
            params: AuthorizationDelegatesParams {
                delegates: delegates.to_vec(),
            },
        }
        .data(),
        program_id: psyche_solana_authorizer::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[grantee])
        .await
}

pub async fn process_authorizer_authorization_revoke(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    grantor: &Keypair,
    grantee: &Pubkey,
    scope: &[u8],
    spill: &Pubkey,
) -> Result<Signature, ToolboxEndpointError> {
    let authorization = find_authorization(&grantor.pubkey(), grantee, scope);

    let accounts = AuthorizationRevokeAccounts {
        grantor: grantor.pubkey(),
        spill: *spill,
        authorization,
        system_program: system_program::ID,
    };
    let instruction = Instruction {
        accounts: accounts.to_account_metas(None),
        data: AuthorizationRevoke {
            params: AuthorizationRevokeParams {},
        }
        .data(),
        program_id: psyche_solana_authorizer::ID,
    };

    endpoint
        .process_instruction_with_signers(instruction, payer, &[grantor])
        .await
}
