use super::*;

#[test]
fn hardware_gpu_adapter_type_predicate_rejects_cpu_and_unknown() {
    assert!(is_hardware_gpu_adapter_type(
        wgpu::DeviceType::IntegratedGpu
    ));
    assert!(is_hardware_gpu_adapter_type(wgpu::DeviceType::DiscreteGpu));
    assert!(is_hardware_gpu_adapter_type(wgpu::DeviceType::VirtualGpu));
    assert!(!is_hardware_gpu_adapter_type(wgpu::DeviceType::Cpu));
    assert!(!is_hardware_gpu_adapter_type(wgpu::DeviceType::Other));
}

#[test]
fn root_group_cpu_oracle_distinguishes_group_from_per_op_opacity() {
    let first = premultiply(ROOT_GROUP_FIRST_COLOR);
    let second = premultiply(ROOT_GROUP_SECOND_COLOR);
    let correct =
        premultiplied_to_readback_rgba8(scale_premultiplied(source_over(second, first), 0.5));
    let incorrectly_baked_per_op = premultiplied_to_readback_rgba8(source_over(
        scale_premultiplied(second, 0.5),
        scale_premultiplied(first, 0.5),
    ));
    assert_ne!(correct, incorrectly_baked_per_op);
    assert_ne!(correct[3], incorrectly_baked_per_op[3]);
}

#[test]
fn readback_padding_roundtrip_uses_non_aligned_rows() {
    let unpadded = WIDTH * BYTES_PER_PIXEL;
    let padded = padded_bytes_per_row(WIDTH);
    assert_ne!(unpadded % COPY_BYTES_PER_ROW_ALIGNMENT, 0);
    assert!(padded > unpadded);
    assert_eq!(unpadded, 268);
    assert_eq!(padded, 512);

    let height = 3;
    let mut mapped = vec![0xee; (padded * height) as usize];
    let mut expected = Vec::with_capacity((unpadded * height) as usize);
    for row in 0..height {
        let payload = (0..unpadded)
            .map(|column| (row.wrapping_mul(37).wrapping_add(column) & 0xff) as u8)
            .collect::<Vec<_>>();
        let start = (row * padded) as usize;
        mapped[start..start + unpadded as usize].copy_from_slice(&payload);
        expected.extend_from_slice(&payload);
    }
    let unpacked =
        remove_row_padding(&mapped, WIDTH, height, padded).expect("valid padded rows must unpack");
    assert_eq!(unpacked, expected);
}
