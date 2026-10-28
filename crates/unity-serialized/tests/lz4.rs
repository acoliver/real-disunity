use worldrunner_unity_serialized::{SerializedError, lz4::decompress_block, shader::CompiledBlobs};

#[test]
fn decompresses_literal_only_blocks() {
    assert_eq!(
        decompress_block(&[0x50, b'h', b'e', b'l', b'l', b'o'], 5).unwrap(),
        b"hello"
    );
    assert_eq!(decompress_block(&[0x00], 0).unwrap(), b"");
}

#[test]
fn copies_matches_that_overlap_their_own_output() {
    // Two literals, then a match of 6 from 2 back, then an empty final sequence.
    let block = [0x22, b'a', b'b', 0x02, 0x00, 0x00];
    assert_eq!(decompress_block(&block, 8).unwrap(), b"abababab");
    // A match from far enough back is a plain copy.
    let block = [0x40, b'w', b'x', b'y', b'z', 0x04, 0x00, 0x10, b'!'];
    assert_eq!(decompress_block(&block, 9).unwrap(), b"wxyzwxyz!");
}

#[test]
fn extends_lengths_with_bytes_until_one_is_not_255() {
    // 15 + 5 literals.
    let mut block = vec![0xf0, 5];
    block.extend(b'a'..b'a' + 20);
    let expected: Vec<u8> = (b'a'..b'a' + 20).collect();
    assert_eq!(decompress_block(&block, 20).unwrap(), expected);
    // One literal, then a match of 4 + 15 + 255 + 1 from 1 back.
    let block = [0x1f, b'x', 0x01, 0x00, 0xff, 0x01, 0x00];
    assert_eq!(decompress_block(&block, 276).unwrap(), vec![b'x'; 276]);
}

fn reason(result: Result<Vec<u8>, SerializedError>) -> &'static str {
    match result.unwrap_err() {
        SerializedError::InvalidCompressedBlock { reason, .. } => reason,
        other => panic!("unexpected error {other}"),
    }
}

#[test]
fn rejects_matches_outside_the_output() {
    let zero = [0x10, b'a', 0x00, 0x00, 0x00];
    assert_eq!(
        reason(decompress_block(&zero, 5)),
        "the match refers back before the start of the output"
    );
    let too_far = [0x10, b'a', 0x02, 0x00, 0x00];
    assert_eq!(
        reason(decompress_block(&too_far, 5)),
        "the match refers back before the start of the output"
    );
}

#[test]
fn rejects_output_of_the_wrong_length() {
    let block = [0x22, b'a', b'b', 0x02, 0x00, 0x00];
    assert_eq!(
        reason(decompress_block(&block, 7)),
        "the output exceeds the declared size"
    );
    assert_eq!(
        reason(decompress_block(&block, 9)),
        "the output is shorter than the declared size"
    );
    assert_eq!(
        reason(decompress_block(&[0x50, b'h', b'e', b'l', b'l', b'o'], 4)),
        "the output exceeds the declared size"
    );
    assert_eq!(
        reason(decompress_block(&[0x00], 256)),
        "the declared size is more than the block can produce"
    );
}

#[test]
fn rejects_truncated_blocks() {
    let block = [0x22, b'a', b'b', 0x02, 0x00, 0x00];
    for end in [0, 2, 4] {
        let error = decompress_block(&block[..end], 8).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCompressedBlock { .. }
            ),
            "prefix {end}: {error}"
        );
    }
    assert!(matches!(
        decompress_block(&[0xf0], 20).unwrap_err(),
        SerializedError::Truncated { .. }
    ));
    assert_eq!(
        reason(decompress_block(&[0x50, b'h', b'i'], 5)),
        "the literals run past the end of the block"
    );
}

/// An object with eight bytes of other fields, then a blob holding two chunks for one platform.
fn object_with_chunks() -> (Vec<u8>, CompiledBlobs) {
    let first = [0x30, b'o', b'n', b'e'];
    let second = [0x12, b't', 0x01, 0x00, 0x00];
    let mut object = vec![0xee; 8];
    object.extend(first);
    object.extend(second);
    let blobs = CompiledBlobs {
        platforms: vec![18],
        offsets: vec![vec![0, 4]],
        compressed_lengths: vec![vec![4, 5]],
        decompressed_lengths: vec![vec![3, 7]],
        blob: 8..17,
        stage_counts: vec![],
    };
    (object, blobs)
}

#[test]
fn decompresses_each_chunk_of_a_platform() {
    let (object, blobs) = object_with_chunks();
    let chunks = blobs.platform_chunks(&object, 0).unwrap();
    assert_eq!(chunks, vec![b"one".to_vec(), b"ttttttt".to_vec()]);
}

fn blob_reason(blobs: &CompiledBlobs, object: &[u8], platform: usize) -> (usize, &'static str) {
    match blobs.platform_chunks(object, platform).unwrap_err() {
        SerializedError::InvalidShaderBlob { chunk, reason, .. } => (chunk, reason),
        other => panic!("unexpected error {other}"),
    }
}

#[test]
fn rejects_platforms_and_chunks_that_do_not_fit() {
    let (object, blobs) = object_with_chunks();
    assert_eq!(
        blob_reason(&blobs, &object, 1),
        (0, "the shader has no such platform")
    );
    let mut uneven = blobs.clone();
    uneven.decompressed_lengths = vec![vec![3]];
    assert_eq!(
        blob_reason(&uneven, &object, 0),
        (0, "the chunk offset and length lists differ in length")
    );
    let mut outside = blobs.clone();
    outside.compressed_lengths = vec![vec![4, 6]];
    assert_eq!(
        blob_reason(&outside, &object, 0),
        (1, "the chunk lies outside the blob")
    );
    let mut past_object = blobs.clone();
    past_object.blob = 8..18;
    assert_eq!(
        blob_reason(&past_object, &object, 0),
        (0, "the blob range lies outside the object")
    );
    let mut wrong_length = blobs;
    wrong_length.decompressed_lengths = vec![vec![3, 6]];
    assert!(matches!(
        wrong_length.platform_chunks(&object, 0).unwrap_err(),
        SerializedError::InvalidCompressedBlock { .. }
    ));
}
