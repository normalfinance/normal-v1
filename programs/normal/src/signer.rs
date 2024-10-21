pub fn get_signer_seeds(nonce: &u8) -> [&[u8]; 2] {
    [b"normal_signer".as_ref(), bytemuck::bytes_of(nonce)]
}
