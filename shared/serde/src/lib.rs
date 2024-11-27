use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_attribute]
pub fn derive_serialize(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let _name = &input.ident;

    let expanded = quote! {
        #[cfg_attr(target_os = "solana", derive(AnchorSerialize, AnchorDeserialize, InitSpace))]
        #[cfg_attr(not(target_os = "solana"), derive(Serialize, Deserialize))]
        #input
    };

    TokenStream::from(expanded)
}
