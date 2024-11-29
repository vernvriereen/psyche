use anchor_lang::prelude::*;


#[account(zero_copy)]
#[repr(C)]
pub struct ClientId {
    pub owner: Pubkey,

}