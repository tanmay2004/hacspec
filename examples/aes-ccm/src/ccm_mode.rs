use hacspec_lib::*;
use hacspec_aes::*;

fn format_func(a: &ByteSeq, n: &ByteSeq, p: &ByteSeq, t: u8, alen: u64, nlen: u8, plen: u64) -> ByteSeq {
    let mut r = 0u64;
    let mut a_octets = 10u64;

    if alen < 0x800000u64 {
        a_octets = 2u64;
    } else {
        if alen < 0x100000000u64 {
            a_octets = 6u64;
        }
    }

    r = r + ((a_octets+alen+15u64)/16u64)+((plen+15u64)/16u64); // ceiling operation used
    let mut b = ByteSeq::new((16u64*(r+1u64)) as usize);

    // creation of b(0)
    let qlen: u8 = 15u8 - nlen;
    let mut flags: u8 = 0u8;

    if alen > 0u64 {
        flags = 0x40u8;
    }

    flags = flags | (((t-2u8)/2u8) << 3);
    flags = flags | (qlen-1u8);
    b[0] = U8(flags);

    for i in 0..(nlen as usize) {
        b[i+1] = n[i];
    }

    let mut plen_copy = plen;

    for i in 1..(qlen as usize)+1 {
        let curr = (plen_copy & 0xFFu64) as u8;
        b[16-i] = U8(curr);
        plen_copy = plen_copy >> 8;
    }

    // creation of b(1) to b(u)
    let x = 0xffu8;
    let y = 0xfeu8;

    let mut k = 16u64; // byte number to set next
    let mut alen_copy = alen;

    if alen >= 0x800000u64 {
        b[16] = U8(x);
        k = k + 2u64;

        if alen < 0x100000000u64 {
            b[17] = U8(y);
        } else {
            b[17] = U8(x);
        }
    }

    for i in 1..(a_octets as usize)+1 {
        let curr = (alen_copy & 0xFFu64) as u8;
        b[((k+a_octets) as usize)-i] = U8(curr);
        alen_copy = alen_copy >> 8;
    }

    k = k + a_octets;

    for i in 0..(alen as usize) {
        b[i+(k as usize)] = a[i];
    }

    k = k + alen;

    for _t in 0..16 {
        if k % 16u64 != 0u64 {
            // add zero padding for Associated Data
            b[k as usize] = U8(0u8);
            k = k + 1u64;
        }
    }

    // creation of b(u+1) to b(r)
    for i in 0..(plen as usize) {
        b[i+(k as usize)] = p[i];
    }

    k = k + plen;

    for _t in 0..16 {
        if k % 16u64 != 0u64 {
            // add zero padding for Payload
            b[k as usize] = U8(0u8);
            k = k + 1u64;
        }
    }

    b
}

fn get_t(b: &ByteSeq, key: Key128, num: u8) -> ByteSeq {
    let b0 = b.get_exact_chunk(16, 0);
    let bloc = Block::from_seq(&b0);
    let mut y_curr = aes128_encrypt_block(key, bloc);

    for i in 1..b.len()/16 {
        let mut b_curr = Block::from_seq(&b.get_exact_chunk(16, i));
        b_curr = y_curr ^ b_curr;
        y_curr = aes128_encrypt_block(key, b_curr);
    }

    ByteSeq::from_seq(&(y_curr.slice(0, num as usize)))
}

fn counter_func(n: &ByteSeq, nlen: u8, m: u64) -> ByteSeq {
    let mut ctr = ByteSeq::new((16u64 * (m+1u64)) as usize);
    let qlen: u8 = 15u8 - nlen;
    let flag = qlen - 1u8;

    for i in 0..(m as usize)+1 {
        let mut icopy = i as u64;

        let k = 16 * i;
        ctr[k] = U8(flag);

        for j in 0..(nlen as usize) {
            ctr[((k+j) as usize)+1] = n[j];
        }

        for x in 1..16-(nlen as usize)-1 {
            let curr = icopy & 0xFFu64;
            ctr[k+16-x] = U8(curr as u8);
            icopy = icopy >> 8;
        }
    }

    ctr
}

fn ctr_cipher(ctr: &ByteSeq, key: Key128, m: u64) -> (ByteSeq, ByteSeq) {
    let ctr_zero = Block::from_seq(&ctr.get_exact_chunk(16, 0));
    let key_copy = key.clone();
    let s0 = ByteSeq::from_seq(&aes128_encrypt_block(key, ctr_zero));
    let mut s = ByteSeq::new((16u64*m) as usize);

    for i in 1..(m as usize)+1 {
        let new_copy = key_copy.clone();
        let ctr_block = Block::from_seq(&ctr.get_exact_chunk(16, i));
        let s_curr = aes128_encrypt_block(new_copy, ctr_block);
        let seq_s = ByteSeq::from_seq(&s_curr);
        s = s.set_exact_chunk(16, i-1, &seq_s);
    }

    (s0, s)
}

pub fn encrypt_ccm(a: ByteSeq, n: ByteSeq, pay: ByteSeq, key: Key128, tlen: u8, alen: u64, nlen: u8, plen: u64) -> ByteSeq {
    let key_copy = key.clone();
    let b = format_func(&a, &n, &pay, tlen, alen, nlen, plen); // step 1
    let t = get_t(&b, key, tlen); // steps 2 to 4

    let m = (plen+15u64)/16u64; // round up
    let counter = counter_func(&n, nlen, m);
    let (s0, s) = ctr_cipher(&counter, key_copy, m);

    let cipherlen = t.len()+pay.len(); let pl = pay.len();
    let mut ciphertext = ByteSeq::new(cipherlen);

    let pay_xor = pay ^ s.get_exact_chunk(plen as usize, 0);
    ciphertext = ciphertext.set_exact_chunk(plen as usize, 0, &pay_xor);

    let t_xor = t ^ s0.get_exact_chunk(tlen as usize, 0);

    for i in pl..cipherlen {
        ciphertext[i] = t_xor[i-pl];
    }

    ciphertext
}

pub fn decrypt_ccm(adata: ByteSeq, nonce: ByteSeq, ciph: ByteSeq, clen: u8, key: Key128, tlen: u8, nlen: u8) -> ByteSeq {
    if clen > tlen {
        let m = (clen-tlen+15u8) / 16u8;
        let counter = counter_func(&nonce, nlen, m as u64);
        let (s0, s) = ctr_cipher(&counter, key, m as u64);

        let x = (clen - tlen) as usize;
        let p = ciph.get_exact_chunk(x, 0) ^ s.get_exact_chunk(x, 0);
        p
    } else {
        ByteSeq::new(0) // TODO: Return "Invalid" instead
    }
}
