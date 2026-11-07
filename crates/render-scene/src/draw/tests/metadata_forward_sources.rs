//! Complete forward source assemblies, including their final screen-space and terrain rewrites.

// Expected byte lengths and FNV-1a/128 were regenerated with the production assembly
// functions below for staged forward rejection (Hubnot 6199655cef5e).
// The refreshed table still guards against unintended forward source changes.
// Each row is object, terrain, then conventional/cached/layered/cached-layered terrain.
const FINAL_FORWARD_SOURCES: &[[(usize, u128); 6]] = &[
    // ordinary velocity=false ssao=false receiver=Nearest
    [
        (92_613, 0xdb10_cc54_7145_1b02_22e4_f07c_7455_f5ae),
        (48_111, 0x375a_5a96_35f7_5f92_c33b_f52a_0571_01a5),
        (50_522, 0xf6e0_39fa_d087_f091_80ff_bad6_de06_4c9e),
        (55_918, 0x50c0_c4b1_33c0_e04b_f620_3605_b9fd_79a7),
        (62_241, 0x3d53_358f_dd9b_f34a_c104_f3de_136e_6848),
        (67_637, 0x56d5_c425_c73d_d8c4_6ba7_d015_c6e5_528d),
    ],
    // ordinary velocity=false ssao=true receiver=Nearest
    [
        (93_032, 0xed11_b6ec_7e80_e8c9_cf71_a55b_76c8_22aa),
        (48_530, 0x57f2_243b_15e3_862e_9ecd_b50d_d5c0_bd57),
        (50_941, 0x7533_3c98_c547_4dde_6aba_72e3_15f1_662a),
        (56_337, 0xf3f4_57d2_452a_4b09_0e0f_43c7_a191_c281),
        (62_660, 0xc3a4_ced1_9b11_3cd5_8640_04df_9a83_3892),
        (68_056, 0xcf7d_4357_088f_7552_8c19_1acb_b3ce_d899),
    ],
    // ordinary velocity=false ssao=true receiver=DepthAwareGather
    [
        (94_426, 0x875f_db96_8909_00b7_6bb0_7387_7191_53b9),
        (49_924, 0xa9a7_62da_5c59_f104_7cd1_7bb1_d433_8354),
        (52_335, 0x79a3_30de_744c_8e8e_d59e_d0c9_79e4_43c5),
        (57_731, 0x4b61_182d_265d_7b4e_3ac2_1380_85fd_eab4),
        (64_054, 0x915a_6227_41b1_2559_9253_fb19_1fe6_be7d),
        (69_450, 0x0f80_5249_8ce8_45eb_d187_03c1_db88_8834),
    ],
    // ordinary velocity=true ssao=false receiver=Nearest
    [
        (96_647, 0x0eb6_e282_e118_8215_b925_90e3_05eb_9d94),
        (48_474, 0x397f_d818_c657_4a48_3eb2_7d9e_fcd3_5f2f),
        (50_885, 0x9c5b_bf90_d47f_8ff1_78b9_99d7_9100_c9e2),
        (56_281, 0x8256_b39d_14a9_0d6d_712e_78b2_0026_a24f),
        (62_604, 0x3469_2610_6052_0958_96b1_49bf_1991_ee78),
        (68_000, 0x256a_398b_4c30_b7c6_4919_79f3_c29b_5e39),
    ],
    // ordinary velocity=true ssao=true receiver=Nearest
    [
        (97_066, 0x0ccd_b7ff_d6e4_ccd4_2e82_4c86_fada_8424),
        (48_893, 0x3d36_e066_ed24_a77b_4f2c_a516_4e8b_6a3b),
        (51_304, 0x0c17_32c9_e8f9_6610_624a_9743_d6b8_5040),
        (56_700, 0x8b20_1f88_5e68_07bc_6cd0_47c6_e323_c1ef),
        (63_023, 0x6d61_fd57_b7ab_969a_3ae9_f31c_00c5_4378),
        (68_419, 0xb51d_7c25_4734_6f2c_9ea6_a82b_7f2b_71a7),
    ],
    // ordinary velocity=true ssao=true receiver=DepthAwareGather
    [
        (98_460, 0x5295_727e_50cd_599f_2de6_da99_4820_f037),
        (50_287, 0x6adc_e337_19d6_b234_e9d1_ebdf_69e3_63a0),
        (52_698, 0xbd2e_fbc8_0135_71c1_9b36_e941_c454_3a07),
        (58_094, 0x8178_1931_d770_0c45_e580_4742_a0bb_821e),
        (64_417, 0xd7c9_3f17_c61b_e9eb_f2e7_ebca_aacf_8377),
        (69_813, 0xff2f_8b29_f008_9018_170a_8ed2_396c_d156),
    ],
    // receiver ssao=false reflection=false velocity=false receiver=Nearest
    [
        (105_814, 0x09ba_62a1_7be3_6d3c_1a20_2ca1_d93d_c0ac),
        (61_312, 0x7527_e442_7bc5_d4b3_b822_e1e6_adc9_e86b),
        (63_723, 0xb28d_36b0_e790_6f36_3d57_c43c_f582_ffd2),
        (69_119, 0x0db6_c8c6_ec76_7afc_2932_b99b_a548_4c9b),
        (75_442, 0x300e_7081_c403_bee0_4b4e_3b7e_853c_823c),
        (80_838, 0xa6c4_e028_5a17_3747_203e_6c82_4109_e0d1),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (109_848, 0x6f4b_92c5_3ffb_1f1c_aeaf_dfde_1081_ceb2),
        (61_675, 0xea56_d43b_f49c_9bbe_b4ff_67dc_f992_a769),
        (64_086, 0xe94d_ab0c_eaec_82f8_9e1d_4ea7_4ce6_98ae),
        (69_482, 0x0682_ed88_9072_6c96_a777_e60c_d755_ee8b),
        (75_805, 0xbead_b94e_0f8f_d7c1_053a_ee0e_75b3_f494),
        (81_201, 0xec93_dc3b_7cb9_7de8_5bc5_7105_6403_a145),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (109_336, 0x1523_9a22_7cf3_a26a_c011_979f_eb2b_39a2),
        (63_776, 0x09f6_7810_1a1f_a764_0355_8d26_4fb1_063a),
        (66_187, 0x42e6_e1e0_e0a0_a005_5e36_3c56_6a11_305b),
        (71_583, 0xffa9_b1ee_5d0a_1f81_cb3c_3edf_aac4_7ebe),
        (77_906, 0x48dc_a752_0511_1995_ef2a_b837_8c18_49ab),
        (83_302, 0xce0d_6c3d_b801_082a_22be_8286_64e9_79a6),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (110_730, 0x80e6_5792_fc27_d5b1_52c6_865b_c1e2_1df9),
        (65_170, 0xf688_d091_d882_138e_1dc2_f4df_e2fc_4c9d),
        (67_581, 0x3e97_e151_882a_2306_b350_32c4_cb4a_5f38),
        (72_977, 0xe6cc_5826_757f_469b_0a82_da93_63c8_1cdb),
        (79_300, 0x950b_5a66_86ce_9f68_2f16_17f5_c129_4160),
        (84_696, 0x9b64_cbed_6a39_43ea_71e6_fb9d_91ed_6b23),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (113_684, 0xaa32_59f4_f310_09ee_79f2_5e43_824f_f8c0),
        (64_133, 0x2880_3350_944a_5f78_2e40_575d_7762_01f7),
        (66_544, 0x6d70_f881_9b6f_0dc9_e5a3_3bb3_2d71_930c),
        (71_940, 0x1735_b04f_e6f7_58d7_d023_fa9e_804b_77a3),
        (78_263, 0x5b00_e564_7908_9ac3_7605_77d4_d3ad_9f34),
        (83_659, 0x7000_a968_53bf_b1cd_6bde_3a34_f7e4_eeeb),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (115_078, 0x18cd_710b_0f11_c426_4bcd_2b18_b027_3889),
        (65_527, 0x5913_2c71_50ac_2647_152f_ae03_d862_e9d2),
        (67_938, 0xacf0_85ae_3da4_6ad6_fa5b_9337_e9cf_4b75),
        (73_334, 0xca0e_90ac_846d_9b70_7af2_0286_c0ad_1ddc),
        (79_657, 0xc241_6dbb_ac73_d085_4462_ae27_471e_49b5),
        (85_053, 0x8bcc_5c88_f4b0_e548_63cb_0ae5_7b73_7884),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (106_233, 0x46ae_6b4d_fc6c_1a78_7bd7_650e_fee5_b162),
        (61_731, 0x1542_aaa3_b8f3_5e2d_4885_0b5f_dc2e_6eef),
        (64_142, 0xe498_e593_6514_b2dd_3c05_6fd3_9768_bb30),
        (69_538, 0xaff2_875b_b51c_381f_e606_3c4e_fd73_14c3),
        (75_861, 0x01f8_ffac_1963_8ba4_5113_82b6_084e_2220),
        (81_257, 0x719f_cde3_bd0c_2f31_fa3e_a5b6_8bf7_75a3),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (107_627, 0xcafe_2dcb_521d_4a0c_b3bf_36b5_e5f4_ae6d),
        (63_125, 0xf157_9a2f_abf4_d922_8279_27ac_a713_f8e0),
        (65_536, 0x3b72_4089_a7fe_37dd_28fc_22b8_79a8_bd83),
        (70_932, 0xb9b3_b4d2_35df_dfad_c13a_c9eb_1a7d_15ae),
        (77_255, 0x6e60_d638_bf1f_a259_1075_81d6_555c_d4db),
        (82_651, 0x8401_cfe0_aa8c_5a47_e81e_d296_23eb_27ce),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (110_267, 0x86b5_c51c_aba5_fc93_73d4_3743_d29e_00cc),
        (62_094, 0x951d_cc39_ffd3_a54e_721f_d28a_0db9_6853),
        (64_505, 0xb27a_e2fd_594a_a85b_4037_5ac9_b548_b28a),
        (69_901, 0x0ec5_5df7_4001_b1ec_64ea_9134_0a15_311d),
        (76_224, 0xb7b7_885b_367c_060e_5820_3da0_e7c3_004a),
        (81_620, 0x0734_5bcb_d715_26bc_54b1_ab7d_c931_c9ed),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (111_661, 0x1b22_b298_ac1e_828c_b0e7_4799_957e_0573),
        (63_488, 0xa903_c5db_241f_9597_4c6d_4f0f_6aeb_20d4),
        (65_899, 0x5c01_4ceb_b304_7df3_1946_c808_2573_87b9),
        (71_295, 0x98cc_be77_074b_db94_bc6c_ca16_7399_cbd4),
        (77_618, 0x80b2_5612_0cdc_731e_cbfc_30b1_307f_a3b9),
        (83_014, 0xd217_aeab_9dbc_8a12_793d_ef06_fbfd_ca5c),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (109_336, 0x1523_9a22_7cf3_a26a_c011_979f_eb2b_39a2),
        (63_776, 0x09f6_7810_1a1f_a764_0355_8d26_4fb1_063a),
        (66_187, 0x42e6_e1e0_e0a0_a005_5e36_3c56_6a11_305b),
        (71_583, 0xffa9_b1ee_5d0a_1f81_cb3c_3edf_aac4_7ebe),
        (77_906, 0x48dc_a752_0511_1995_ef2a_b837_8c18_49ab),
        (83_302, 0xce0d_6c3d_b801_082a_22be_8286_64e9_79a6),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (110_730, 0x80e6_5792_fc27_d5b1_52c6_865b_c1e2_1df9),
        (65_170, 0xf688_d091_d882_138e_1dc2_f4df_e2fc_4c9d),
        (67_581, 0x3e97_e151_882a_2306_b350_32c4_cb4a_5f38),
        (72_977, 0xe6cc_5826_757f_469b_0a82_da93_63c8_1cdb),
        (79_300, 0x950b_5a66_86ce_9f68_2f16_17f5_c129_4160),
        (84_696, 0x9b64_cbed_6a39_43ea_71e6_fb9d_91ed_6b23),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (113_684, 0xaa32_59f4_f310_09ee_79f2_5e43_824f_f8c0),
        (64_133, 0x2880_3350_944a_5f78_2e40_575d_7762_01f7),
        (66_544, 0x6d70_f881_9b6f_0dc9_e5a3_3bb3_2d71_930c),
        (71_940, 0x1735_b04f_e6f7_58d7_d023_fa9e_804b_77a3),
        (78_263, 0x5b00_e564_7908_9ac3_7605_77d4_d3ad_9f34),
        (83_659, 0x7000_a968_53bf_b1cd_6bde_3a34_f7e4_eeeb),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (115_078, 0x18cd_710b_0f11_c426_4bcd_2b18_b027_3889),
        (65_527, 0x5913_2c71_50ac_2647_152f_ae03_d862_e9d2),
        (67_938, 0xacf0_85ae_3da4_6ad6_fa5b_9337_e9cf_4b75),
        (73_334, 0xca0e_90ac_846d_9b70_7af2_0286_c0ad_1ddc),
        (79_657, 0xc241_6dbb_ac73_d085_4462_ae27_471e_49b5),
        (85_053, 0x8bcc_5c88_f4b0_e548_63cb_0ae5_7b73_7884),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=Nearest
    [
        (96_135, 0xc968_1b75_5ee0_4ff3_cc0e_2db8_c26e_ee08),
        (50_575, 0xe615_a1e2_7e86_a218_3dd0_79d4_6f17_c016),
        (52_986, 0x2f4a_d48f_81ac_d9fa_2e16_9ffc_3a5f_a77d),
        (58_382, 0xb210_9b09_b4e4_e709_8920_0dfd_a6b6_ea04),
        (64_705, 0x6971_f17e_1f7b_d62c_60ba_c2a4_9833_dabd),
        (70_101, 0x24f3_4429_3f23_bee7_4968_76ff_b35a_015c),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=DepthAwareGather
    [
        (97_529, 0x2a2b_6049_acf4_616e_6598_8462_2f76_e70f),
        (51_969, 0x5814_fcd8_eb94_5fb9_61c9_1ef4_8ddd_5725),
        (54_380, 0x5edd_3ed3_c55a_6503_52e6_6ac5_3ba6_6d0a),
        (59_776, 0x914b_40ab_d9f3_152a_716f_3b3d_f4bb_5481),
        (66_099, 0x88a5_8c57_d938_5529_24c7_bbb9_b67c_5f5a),
        (71_495, 0xc651_130f_1cf6_7cd8_4198_23cb_ebe1_0271),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=Nearest
    [
        (96_201, 0x31a8_4e27_2d66_8c12_d3da_b7be_8c16_2161),
        (50_641, 0xa31d_0acd_26aa_3fc9_b52a_f277_480c_3577),
        (53_052, 0x4588_0155_da69_2c71_6051_5183_b7a8_0a94),
        (58_448, 0xe731_b0e6_ebb0_17e9_f18d_50b6_08e1_1827),
        (64_771, 0x8e19_ecfe_5382_0dec_aa10_52e3_9292_f5c4),
        (70_167, 0xaf7a_73f1_1114_05f9_5301_3640_d0ab_df37),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=DepthAwareGather
    [
        (97_595, 0x016e_372c_7487_7008_5817_aa19_f450_32da),
        (52_035, 0x1596_fc24_9e03_d7ad_a310_6695_44b4_66fc),
        (54_446, 0x6d65_d086_5271_fac0_7d30_17bb_953e_eca3),
        (59_842, 0xdf3d_7d14_b618_0bb6_c620_b3a9_24e4_cb26),
        (66_165, 0x9bb5_603f_df01_c059_f4e5_d4d0_823c_d2ab),
        (71_561, 0xd866_7998_0ba7_947c_9291_7e35_a51c_da96),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=Nearest
    [
        (100_483, 0x59ae_5f69_d1aa_51b0_104d_4707_4c0d_3006),
        (50_932, 0x5fb1_5d20_261e_6ce2_ae84_729f_68d8_e8f3),
        (53_343, 0xcaa4_423e_9520_a1b3_fe43_7b4e_d9c1_b62e),
        (58_739, 0xd261_fbcc_2630_f877_ff11_3e04_5ced_5369),
        (65_062, 0x5d62_b240_2f3b_47cd_c3e7_177b_baf0_27ce),
        (70_458, 0xb9e9_b9a4_09dd_ac2b_956c_f945_d41b_66d9),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=DepthAwareGather
    [
        (101_877, 0xa94b_5d5b_efab_7344_c909_f0ce_9fa7_993f),
        (52_326, 0xb492_409e_1a59_f80c_eae7_73ff_c787_5f5a),
        (54_737, 0xe32c_9656_d4c7_06c3_434e_6eb7_dbd2_5f2f),
        (60_133, 0xd22f_a126_6e7b_2732_8bba_856d_c120_dd02),
        (66_456, 0x4cef_7605_79cb_d840_99a2_c1b9_43fc_f29f),
        (71_852, 0xeb7f_a09b_6354_8fc7_a3ba_ed94_1f4c_c83a),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=Nearest
    [
        (100_549, 0x47d9_f29f_4ba6_9992_2be5_5d57_223c_f58d),
        (50_998, 0xeb0a_c269_756a_6cfa_7799_4ec4_a48e_722e),
        (53_409, 0x3778_a735_3d4a_2c62_a9f5_8c05_5ad1_ef6b),
        (58_805, 0xeb66_86e8_cb66_3f62_b46b_92bb_0a53_030a),
        (65_128, 0x92bc_9086_d735_3e8d_ff9e_cc02_4124_a7f3),
        (70_524, 0x2429_d862_639a_dc3c_9256_7806_929b_9f7a),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=DepthAwareGather
    [
        (101_943, 0x7b62_c0a7_f58d_4896_0070_8354_d687_e2f8),
        (52_392, 0xc985_6ebc_e1c4_c12b_1c2e_2250_8dc2_85d7),
        (54_803, 0x673d_f08a_9401_9bb2_d6be_a1dc_08ad_30da),
        (60_199, 0xe7f6_4a76_2f75_35a7_e3e0_0ac5_4604_fa21),
        (66_522, 0xdada_a6a2_8e38_5e9d_6079_e8eb_c182_2cf2),
        (71_918, 0x8c9f_8207_beb4_f0d5_8bb1_e1f3_8803_aa39),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=Nearest
    [
        (109_336, 0x1523_9a22_7cf3_a26a_c011_979f_eb2b_39a2),
        (63_776, 0x09f6_7810_1a1f_a764_0355_8d26_4fb1_063a),
        (66_187, 0x42e6_e1e0_e0a0_a005_5e36_3c56_6a11_305b),
        (71_583, 0xffa9_b1ee_5d0a_1f81_cb3c_3edf_aac4_7ebe),
        (77_906, 0x48dc_a752_0511_1995_ef2a_b837_8c18_49ab),
        (83_302, 0xce0d_6c3d_b801_082a_22be_8286_64e9_79a6),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (110_730, 0x80e6_5792_fc27_d5b1_52c6_865b_c1e2_1df9),
        (65_170, 0xf688_d091_d882_138e_1dc2_f4df_e2fc_4c9d),
        (67_581, 0x3e97_e151_882a_2306_b350_32c4_cb4a_5f38),
        (72_977, 0xe6cc_5826_757f_469b_0a82_da93_63c8_1cdb),
        (79_300, 0x950b_5a66_86ce_9f68_2f16_17f5_c129_4160),
        (84_696, 0x9b64_cbed_6a39_43ea_71e6_fb9d_91ed_6b23),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (109_402, 0xc531_b64b_5c87_b333_6ab8_9966_c3dd_c383),
        (63_842, 0xd43e_ca51_485a_88a9_8f51_1c36_dbbf_db23),
        (66_253, 0xf33b_5d28_2128_86ca_e136_d0f6_521c_cf2a),
        (71_649, 0xe29e_cf28_7b0c_6edc_3519_2070_7e1f_4421),
        (77_972, 0x8906_5298_b544_86e6_8d5a_b4f9_c282_2562),
        (83_368, 0x19c7_d492_02f1_d5a9_dc5d_45ed_8d21_2a09),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (110_796, 0x7028_0714_61e7_6de4_bb0c_250b_2be4_3c4c),
        (65_236, 0x5bd9_02ed_bf58_a7fb_beab_444d_bc86_7724),
        (67_647, 0x0c75_dc9d_bedd_4a89_f7e6_21cb_e324_28d1),
        (73_043, 0xf288_0a23_b38c_a0a6_a76b_9bfa_40c5_b248),
        (79_366, 0x9667_4784_a147_9726_1810_c2a6_447e_ecc9),
        (84_762, 0x4af4_0171_d435_4c85_7299_fa3f_7bf2_d0c8),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (113_684, 0xaa32_59f4_f310_09ee_79f2_5e43_824f_f8c0),
        (64_133, 0x2880_3350_944a_5f78_2e40_575d_7762_01f7),
        (66_544, 0x6d70_f881_9b6f_0dc9_e5a3_3bb3_2d71_930c),
        (71_940, 0x1735_b04f_e6f7_58d7_d023_fa9e_804b_77a3),
        (78_263, 0x5b00_e564_7908_9ac3_7605_77d4_d3ad_9f34),
        (83_659, 0x7000_a968_53bf_b1cd_6bde_3a34_f7e4_eeeb),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (115_078, 0x18cd_710b_0f11_c426_4bcd_2b18_b027_3889),
        (65_527, 0x5913_2c71_50ac_2647_152f_ae03_d862_e9d2),
        (67_938, 0xacf0_85ae_3da4_6ad6_fa5b_9337_e9cf_4b75),
        (73_334, 0xca0e_90ac_846d_9b70_7af2_0286_c0ad_1ddc),
        (79_657, 0xc241_6dbb_ac73_d085_4462_ae27_471e_49b5),
        (85_053, 0x8bcc_5c88_f4b0_e548_63cb_0ae5_7b73_7884),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (113_750, 0x54f0_b7a1_74a2_ae58_67d3_ac8a_b2f0_de1f),
        (64_199, 0x6f42_a0dd_033b_99a9_071a_dab1_8b2b_e4ba),
        (66_610, 0x7646_bae7_7fb2_bf30_2b0d_e172_77be_9ba1),
        (72_006, 0xd07c_b1f4_33dd_4723_f272_db1e_e67b_6dc4),
        (78_329, 0xf001_6a92_0f2f_5443_d43f_f309_b8ca_f339),
        (83_725, 0x984e_78af_abe1_48e9_0a4e_1745_39c5_b7e4),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (115_144, 0x4620_77ee_d2de_74d5_57fb_c592_28e6_0b82),
        (65_593, 0xcf7f_5e86_f542_6920_d84a_2696_1c73_53ff),
        (68_004, 0x73d8_71de_a799_fdce_aec5_02b1_fd1f_71d8),
        (73_400, 0x0da1_47dc_129c_3b5f_a475_6cbf_0138_9e1b),
        (79_723, 0xd29a_eda9_f5ac_fcfe_ffbb_127a_b498_2b88),
        (85_119, 0x8371_9699_0f5c_1c2e_8d28_aba0_77d6_29eb),
    ],
];

fn forward_source_cases() -> Vec<(String, (String, String))> {
    let stock = (crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL);
    let mut cases = Vec::new();
    for velocity in [false, true] {
        for ssao in [false, true] {
            let (object, terrain) = if velocity {
                (
                    crate::velocity::object_source(stock.0),
                    crate::velocity::terrain_source(stock.1),
                )
            } else {
                (stock.0.to_owned(), stock.1.to_owned())
            };
            let sources = if ssao {
                (
                    crate::ssao::ambient_source(&object),
                    crate::ssao::ambient_source(&terrain),
                )
            } else {
                (object, terrain)
            };
            cases.push((format!("ordinary velocity={velocity} ssao={ssao}"), sources));
        }
    }
    for ssao in [false, true] {
        for reflection in [false, true] {
            for velocity in [false, true] {
                cases.push((
                    format!("receiver ssao={ssao} reflection={reflection} velocity={velocity}"),
                    crate::shadows::receiver_sources(ssao, reflection, velocity),
                ));
            }
        }
    }
    for shadowed in [false, true] {
        let stock = crate::shadows::capture_sources(shadowed);
        for velocity in [false, true] {
            for application_ao in [false, true] {
                cases.push((
                    format!("ssr shadowed={shadowed} velocity={velocity} application_ao={application_ao}"),
                    crate::ssr::forward::velocity_sources(stock, velocity, application_ao),
                ));
            }
        }
    }
    cases
}

fn final_forward_sources() -> Vec<(String, [String; 6])> {
    use worldrunner_render_plan::SceneSsaoReceiver::{DepthAwareGather, Nearest};
    let mut cases = Vec::new();
    for (label, (object, terrain)) in forward_source_cases() {
        let receivers: &[_] = if object.contains("fn screen_ao(") {
            &[Nearest, DepthAwareGather]
        } else {
            &[Nearest]
        };
        for &receiver in receivers {
            let object = crate::ssao::receiver::source(&object, receiver);
            let terrain = crate::ssao::receiver::source(&terrain, receiver);
            let [plain, cached, layered, layered_cached] =
                crate::renderer::terrain_test_sources(&terrain);
            cases.push((
                format!("{label} receiver={receiver:?}"),
                [object, terrain, plain, cached, layered, layered_cached],
            ));
        }
    }
    cases
}

#[test]
fn final_forward_assemblies_preserve_complete_pre_change_source() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let cases = final_forward_sources();
    assert_eq!(cases.len(), FINAL_FORWARD_SOURCES.len());
    for ((label, sources), expected) in cases.into_iter().zip(FINAL_FORWARD_SOURCES) {
        for (index, (source, expected)) in sources.iter().zip(expected).enumerate() {
            assert_eq!(
                super::source_fingerprint(source),
                *expected,
                "{label} source={index}"
            );
        }
    }
}
