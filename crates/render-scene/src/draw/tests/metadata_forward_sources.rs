//! Complete forward source assemblies, including their final screen-space and terrain rewrites.

// Expected byte lengths and FNV-1a/128 from production assembly with invariant positions.
// Each row is object, terrain, then conventional/cached/layered/cached-layered terrain.
const FINAL_FORWARD_SOURCES: &[[(usize, u128); 6]] = &[
    // ordinary velocity=false ssao=false receiver=Nearest
    [
        (93_482, 0x22e8_31dc_934f_cb24_2e4f_660c_23cb_6188),
        (48_980, 0xa055_4b91_69fa_ff16_c593_ee7c_7301_a789),
        (51_391, 0xa5dd_7650_1bc0_1ade_6574_89a1_22b2_48e0),
        (56_972, 0xe5de_5383_90c1_6e84_d315_a393_e73d_3928),
        (63_110, 0xb38d_f7d5_73fa_8bef_fe22_fc29_206a_6686),
        (68_691, 0x0011_1892_465e_79c0_64d3_5c09_e797_5d2e),
    ],
    // ordinary velocity=false ssao=true receiver=Nearest
    [
        (93_901, 0xa129_c0de_4c4b_74bd_fd3f_4320_98ae_81aa),
        (49_399, 0x7134_c45b_920e_b087_63c2_b2f6_3814_dac5),
        (51_810, 0xe25f_ceaa_7706_ce70_dffb_a9e5_9dc2_1a56),
        (57_391, 0x26d7_5127_b6d8_5965_ea65_5235_8ed1_0aa8),
        (63_529, 0x29d6_3068_8200_ffb9_74d3_39e3_7b01_ace6),
        (69_110, 0x0a48_d888_b65a_6c4a_521d_88cb_b22c_26a8),
    ],
    // ordinary velocity=false ssao=true receiver=DepthAwareGather
    [
        (95_295, 0xc6b6_7746_5a6b_38d0_0960_4c28_6184_1701),
        (50_793, 0xc083_7cf2_c1de_4e49_67dd_346e_4d53_9136),
        (53_204, 0x51e9_bd76_e7e0_cfad_2ae2_c03e_836e_e441),
        (58_785, 0x8fc4_f80a_abb3_52fc_3631_370a_fd03_ba2b),
        (64_923, 0xaaae_3f9f_d81f_b098_af56_6e5f_ec4f_8c81),
        (70_504, 0x472c_b6fa_c63a_a334_f52e_58c8_0f64_c1a3),
    ],
    // ordinary velocity=true ssao=false receiver=Nearest
    [
        (97_516, 0x9d48_4777_7456_6105_da55_1b14_468e_f896),
        (49_343, 0x82c0_621d_51b9_75d9_54da_286a_e58e_6a5b),
        (51_754, 0x88d3_051d_2928_78eb_5580_f73d_55c7_fb18),
        (57_335, 0xe350_d369_44e3_1927_73fb_2036_f865_eda4),
        (63_473, 0xf60f_4c98_f9b4_ba11_e67f_f772_d695_2a02),
        (69_054, 0xc39f_0165_aee7_4eac_7109_dfcd_4d97_755e),
    ],
    // ordinary velocity=true ssao=true receiver=Nearest
    [
        (97_935, 0x4ec9_638b_7def_5d2e_5ae9_5295_2cc4_8504),
        (49_762, 0x8084_44f2_efd7_2f73_506e_fc19_4fbb_cabd),
        (52_173, 0xfb78_f5f4_163d_d203_1d68_9907_2e6b_a584),
        (57_754, 0x0c11_18c8_e992_05c7_38c6_4671_aa6e_de82),
        (63_892, 0x25fd_576e_7d6d_ccd8_cc2e_7290_4224_2f94),
        (69_473, 0xdbe7_bc26_17ad_cfe2_97f4_f79c_765c_1152),
    ],
    // ordinary velocity=true ssao=true receiver=DepthAwareGather
    [
        (99_329, 0xcc1b_19f0_6626_aa1f_3046_95ce_99a1_96bf),
        (51_156, 0xea7e_f475_aa41_07c6_9550_dede_c68d_5b86),
        (53_567, 0xb4be_e6c9_a254_15f9_90d1_f368_0252_dfbb),
        (59_148, 0xc125_8bbd_82cd_9055_a1d8_b13b_614a_54e9),
        (65_286, 0x4f13_e372_e51a_0bc6_ca1c_e40b_2992_1cf3),
        (70_867, 0x24c2_e753_5413_3779_f814_866e_08b8_99f9),
    ],
    // receiver ssao=false reflection=false velocity=false receiver=Nearest
    [
        (109_995, 0x26dc_ac93_c3f2_6e4e_da1d_fd5f_efc7_a06f),
        (67_036, 0x4682_13de_1c13_ba09_47c2_724a_058b_0780),
        (69_447, 0x04d3_08f5_be24_521f_a9e5_aafc_ae17_e45d),
        (75_028, 0xb536_3024_968d_53d7_5f9f_c867_077d_7603),
        (81_166, 0x267b_7479_5414_928b_861d_94d3_6c77_98ad),
        (86_747, 0x77de_9f8e_de0b_546b_965c_af37_aec6_0f7b),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (114_029, 0xa20e_187f_e3c8_3cb2_1778_cc9e_6073_95ed),
        (67_399, 0xcbd3_0732_41e7_2e1d_7d11_e4bd_63e6_1cdc),
        (69_810, 0xe31b_1217_c44c_c563_3595_2295_78ea_2657),
        (75_391, 0x6491_653f_ed13_c8bd_e69f_24ec_0663_8c19),
        (81_529, 0xa2ea_072a_1c13_2154_068f_5ad6_4065_248f),
        (87_110, 0xf2a3_3aa3_5ec7_91e0_a803_20c5_bf6c_9749),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (113_517, 0x6277_05e0_a79c_9b07_2e4a_6f85_5c17_8899),
        (69_500, 0xd1ad_154b_eb0c_526d_5bf0_0408_bc71_9d81),
        (71_911, 0xd2e6_394b_ce82_2e44_3763_f73d_ebfb_4090),
        (77_492, 0x4e3e_8770_a786_565b_1397_37ea_af41_86e8),
        (83_630, 0x935e_943e_3ae9_24f4_7437_3dee_f542_23a6),
        (89_211, 0xb62d_2ec5_6e87_defa_d589_b1c1_9865_88ce),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (114_911, 0xfc6a_44ef_b83b_3fbc_1d51_d958_ff3f_039a),
        (70_894, 0xd861_aeb3_2e8e_9162_8252_55be_52f5_de3a),
        (73_305, 0x0f18_dd68_494a_b140_ec2a_31e3_2894_ef77),
        (78_886, 0x0ac2_0907_46c8_ca1a_dfd9_6a2b_706b_9373),
        (85_024, 0x73b5_1342_ffaf_1a1c_42a4_81c4_f85f_73a1),
        (90_605, 0xc0e2_8d4a_9fe0_ea2e_d4cc_8dde_49e8_492d),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (117_865, 0x1057_366f_ebce_0084_5b86_9c2e_137a_e0e3),
        (69_857, 0x78c0_2009_d0c9_12ba_7aa5_44d9_dc08_4fe2),
        (72_268, 0x57a8_c60f_5a1d_8033_7300_2f7b_38e7_6915),
        (77_849, 0xa952_0114_33b8_7eec_042c_20a0_437a_1525),
        (83_987, 0x8d5a_b0b8_68dc_4c37_0e15_804c_b3f0_d063),
        (89_568, 0x9441_2526_87a7_8cd4_7c42_18bc_f0fb_c38b),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (119_259, 0x43a1_381d_556b_fb78_f7c3_ac07_a826_8b2a),
        (71_251, 0x1375_b6f2_a58c_e31c_355c_5bd7_54d1_c2bf),
        (73_662, 0x03a1_c05f_df4a_c1ea_60a9_8d8a_742e_0664),
        (79_243, 0x79ed_3bd6_553f_91d7_484a_889f_d994_a5d4),
        (85_381, 0xcd30_8033_fd36_3841_67a7_afb6_19a8_6526),
        (90_962, 0xbcce_7403_63cc_d3a4_8c6c_6940_4e26_5e36),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (110_414, 0xfd25_8b89_e4b2_16be_2d85_38b0_44e6_8165),
        (67_455, 0xf2dd_c7db_3b69_413b_50c0_842a_7c4c_9c56),
        (69_866, 0xae3a_3d5d_f31d_61e3_f6bd_4b64_0510_08d1),
        (75_447, 0xef29_c7c1_b63e_d704_1d5c_40c4_2653_27b1),
        (81_585, 0x5ed6_33e5_3d7a_07b3_1670_b368_56f6_9073),
        (87_166, 0x2f86_f615_bfcf_e76e_c7b4_d99f_027e_c65b),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (111_808, 0xdb82_b847_5339_95a0_c20e_2e6f_efe1_2006),
        (68_849, 0xaecc_74d5_4d5a_c247_1c18_0410_5471_be5d),
        (71_260, 0x406c_2bd3_3eaa_6a3d_2132_38dc_abbf_b53e),
        (76_841, 0x8b17_403c_d3b7_932b_7e1a_e0ae_4605_2322),
        (82_979, 0x620f_ae18_d380_d6dc_b803_2467_aa51_0c4c),
        (88_560, 0xaf26_a5f1_c383_5694_7a66_c8c1_e3d4_8a38),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (114_448, 0xc5b4_14e9_b432_63fe_d336_c861_4e14_24b7),
        (67_818, 0x31b7_8395_0c40_e89d_f278_4b66_c3ac_77f4),
        (70_229, 0x4121_15a5_9a33_1303_5b47_95b2_3deb_de75),
        (75_810, 0x4634_5a2b_812e_04c7_ed80_3e42_8217_dbd1),
        (81_948, 0x6c16_2b6d_41ed_8bde_5e4f_bc63_fe45_3193),
        (87_529, 0xdbe6_60fe_0414_fc6d_da81_b5de_e2e2_f8d7),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (115_842, 0xf372_47db_7abf_d774_ec3e_2e0f_0377_fd34),
        (69_212, 0xd53c_f095_5a8e_1647_3662_1846_bf6c_fdef),
        (71_623, 0x69f9_bb3b_8875_0d94_3cca_b4c9_5995_3ef2),
        (77_204, 0xf10f_6ab1_cc2c_c6f3_c1df_7192_1760_32c2),
        (83_342, 0xd78c_bfba_6665_c4a2_98ce_7d35_b715_a7ec),
        (88_923, 0xfb84_2256_8e66_330e_99e9_0b04_8d78_1894),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (113_517, 0x6277_05e0_a79c_9b07_2e4a_6f85_5c17_8899),
        (69_500, 0xd1ad_154b_eb0c_526d_5bf0_0408_bc71_9d81),
        (71_911, 0xd2e6_394b_ce82_2e44_3763_f73d_ebfb_4090),
        (77_492, 0x4e3e_8770_a786_565b_1397_37ea_af41_86e8),
        (83_630, 0x935e_943e_3ae9_24f4_7437_3dee_f542_23a6),
        (89_211, 0xb62d_2ec5_6e87_defa_d589_b1c1_9865_88ce),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (114_911, 0xfc6a_44ef_b83b_3fbc_1d51_d958_ff3f_039a),
        (70_894, 0xd861_aeb3_2e8e_9162_8252_55be_52f5_de3a),
        (73_305, 0x0f18_dd68_494a_b140_ec2a_31e3_2894_ef77),
        (78_886, 0x0ac2_0907_46c8_ca1a_dfd9_6a2b_706b_9373),
        (85_024, 0x73b5_1342_ffaf_1a1c_42a4_81c4_f85f_73a1),
        (90_605, 0xc0e2_8d4a_9fe0_ea2e_d4cc_8dde_49e8_492d),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (117_865, 0x1057_366f_ebce_0084_5b86_9c2e_137a_e0e3),
        (69_857, 0x78c0_2009_d0c9_12ba_7aa5_44d9_dc08_4fe2),
        (72_268, 0x57a8_c60f_5a1d_8033_7300_2f7b_38e7_6915),
        (77_849, 0xa952_0114_33b8_7eec_042c_20a0_437a_1525),
        (83_987, 0x8d5a_b0b8_68dc_4c37_0e15_804c_b3f0_d063),
        (89_568, 0x9441_2526_87a7_8cd4_7c42_18bc_f0fb_c38b),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (119_259, 0x43a1_381d_556b_fb78_f7c3_ac07_a826_8b2a),
        (71_251, 0x1375_b6f2_a58c_e31c_355c_5bd7_54d1_c2bf),
        (73_662, 0x03a1_c05f_df4a_c1ea_60a9_8d8a_742e_0664),
        (79_243, 0x79ed_3bd6_553f_91d7_484a_889f_d994_a5d4),
        (85_381, 0xcd30_8033_fd36_3841_67a7_afb6_19a8_6526),
        (90_962, 0xbcce_7403_63cc_d3a4_8c6c_6940_4e26_5e36),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=Nearest
    [
        (97_004, 0x3671_0718_ad4d_29a2_d276_4bf9_3b21_4892),
        (51_444, 0x02c2_ae4d_4e3e_b225_cebd_c644_8056_22f0),
        (53_855, 0x1994_f033_7173_9f08_5038_830d_d984_6951),
        (59_436, 0x0731_ae86_0e92_79eb_8d6f_cdf1_f44f_47a3),
        (65_574, 0xdcbc_0bbf_799d_4ee9_7671_3f83_2776_80f9),
        (71_155, 0xa5b8_2b28_28e3_2543_367b_3b2b_6b25_74b3),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=DepthAwareGather
    [
        (98_398, 0xac9d_7f89_f8e9_96f6_ba25_9258_1f99_34bd),
        (52_838, 0x2bf2_c3e6_1292_fabf_e766_2efd_dc43_d46b),
        (55_249, 0xffcb_6860_9133_1d0c_2d50_a2fa_b853_8486),
        (60_830, 0x188b_5a51_0c86_3e9f_37a0_72a7_94be_a118),
        (66_968, 0x42b4_2eba_7e83_29be_70b2_4c4b_5e0b_a53e),
        (72_549, 0x6d72_fd9f_3567_9f3c_347d_4c1d_8380_2850),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=Nearest
    [
        (97_070, 0xca78_e2a8_df73_c438_8480_7ec0_2496_7bb3),
        (51_510, 0x8dfb_b306_dc40_952e_aed2_fc75_6a39_d739),
        (53_921, 0x1983_0d7e_b0ae_fa78_9c8c_278c_b622_2040),
        (59_502, 0x7ae3_6afa_56e0_576e_ebe4_065f_c10f_ae62),
        (65_640, 0x60a3_d37f_9e26_b48c_ebfd_8850_ffe9_17e8),
        (71_221, 0xadf4_9ebe_2467_349e_43de_a500_e182_450a),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=DepthAwareGather
    [
        (98_464, 0x000a_3df6_a182_a474_92d1_2c99_2c51_6e38),
        (52_904, 0x3040_c415_029f_ba3b_520d_121c_7ccc_e03a),
        (55_315, 0x83ee_1825_4014_dccc_1280_bc43_5843_96d7),
        (60_896, 0xceb7_8110_084e_40b6_e710_cd14_d70a_88f1),
        (67_034, 0x07be_8e6d_f453_2b72_7c1b_ed56_e4c8_2507),
        (72_615, 0xe66e_cf4e_2b17_b920_2489_dee9_0754_8fb9),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=Nearest
    [
        (101_352, 0x1152_d7d5_378c_be46_e0f8_5f81_b8a5_1560),
        (51_801, 0xf98f_d39e_2d3b_4d55_9633_69fa_4124_7035),
        (54_212, 0x9298_0144_2bc2_7aac_465d_73c2_6c33_bbd2),
        (59_793, 0xcdfb_3681_26fc_df23_535f_6631_04b6_4b08),
        (65_931, 0xb060_0ed2_9782_de00_07c6_c3e1_8959_3f7a),
        (71_512, 0x8ce0_4356_4d93_3637_5505_6ca6_6ad0_3a60),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=DepthAwareGather
    [
        (102_746, 0x80bd_7abc_fdfc_1da7_18d1_78c5_6910_8715),
        (53_195, 0x3aab_a5cb_5485_7e31_15c6_6296_26d5_a698),
        (55_606, 0x95c0_7e88_1578_0a8c_8a6c_d6a1_1593_639b),
        (61_187, 0xb228_9885_3eef_168d_fcb2_9e55_b0a5_373d),
        (67_325, 0xa04b_593c_98ac_be69_4676_d988_f7dd_ce43),
        (72_906, 0xdb81_86a4_ecfe_2688_c7fe_d492_c8b9_c4fd),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=Nearest
    [
        (101_418, 0x241a_48d2_1a34_1ad4_e28c_b428_00fb_e4bf),
        (51_867, 0x0c16_8bb3_422f_cc27_2c74_1220_56af_c2d0),
        (54_278, 0xfb9a_5797_a661_a507_4e83_7fbc_12f3_0287),
        (59_859, 0xfe02_d0ce_9ec1_105c_a9e1_88aa_b26d_9555),
        (65_997, 0x930a_3335_a990_63b7_08ba_b33c_a95f_0617),
        (71_578, 0x8a8c_39ed_806c_a029_32a1_f510_bcf8_6a7d),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=DepthAwareGather
    [
        (102_812, 0x0cea_ed31_b205_5d15_7730_a02f_9918_8166),
        (53_261, 0x79f2_8487_a9a7_105b_ace5_ba19_5e5d_106d),
        (55_672, 0x7654_b93c_5190_78e5_38ad_2dd5_7562_a6be),
        (61_253, 0xe46c_202d_cf1e_e21d_0783_c0db_3c42_a040),
        (67_391, 0x28f8_65a2_8db9_2dac_0a8b_6259_8fff_8e2e),
        (72_972, 0x562f_87d0_47c7_13f0_1443_fed0_70e7_2a30),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=Nearest
    [
        (113_517, 0x6277_05e0_a79c_9b07_2e4a_6f85_5c17_8899),
        (69_500, 0xd1ad_154b_eb0c_526d_5bf0_0408_bc71_9d81),
        (71_911, 0xd2e6_394b_ce82_2e44_3763_f73d_ebfb_4090),
        (77_492, 0x4e3e_8770_a786_565b_1397_37ea_af41_86e8),
        (83_630, 0x935e_943e_3ae9_24f4_7437_3dee_f542_23a6),
        (89_211, 0xb62d_2ec5_6e87_defa_d589_b1c1_9865_88ce),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (114_911, 0xfc6a_44ef_b83b_3fbc_1d51_d958_ff3f_039a),
        (70_894, 0xd861_aeb3_2e8e_9162_8252_55be_52f5_de3a),
        (73_305, 0x0f18_dd68_494a_b140_ec2a_31e3_2894_ef77),
        (78_886, 0x0ac2_0907_46c8_ca1a_dfd9_6a2b_706b_9373),
        (85_024, 0x73b5_1342_ffaf_1a1c_42a4_81c4_f85f_73a1),
        (90_605, 0xc0e2_8d4a_9fe0_ea2e_d4cc_8dde_49e8_492d),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (113_583, 0xc9d9_f51c_30d3_2a51_8554_4c15_31a3_e43c),
        (69_566, 0x6bd4_d21c_f1ee_4ea7_092a_115b_9751_f8f0),
        (71_977, 0x4e4a_7b22_120d_ef0f_4923_e371_0223_19d9),
        (77_558, 0x842e_f0b3_219d_37c2_dc8e_879e_a74c_5751),
        (83_696, 0x428f_dd37_9812_6224_208d_c168_2fd1_9d4f),
        (89_277, 0x1082_3af7_4337_429a_a5d0_0f9a_864f_9ce7),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (114_977, 0x0b5e_38eb_727d_d16e_1153_7f91_c38f_65f3),
        (70_960, 0xe0fd_a52b_d643_ae07_1ac9_d323_3f4a_e1d3),
        (73_371, 0xeba1_ccd2_8c7c_39d9_b050_e469_681e_5b1e),
        (78_952, 0xb0bc_5792_2785_4bcc_6e38_7d09_cd41_a3f2),
        (85_090, 0x29f1_b36c_c936_3be0_8e28_1f74_fbed_b2f0),
        (90_671, 0x2cc4_6886_571e_9937_5e4a_7580_49bf_0b64),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (117_865, 0x1057_366f_ebce_0084_5b86_9c2e_137a_e0e3),
        (69_857, 0x78c0_2009_d0c9_12ba_7aa5_44d9_dc08_4fe2),
        (72_268, 0x57a8_c60f_5a1d_8033_7300_2f7b_38e7_6915),
        (77_849, 0xa952_0114_33b8_7eec_042c_20a0_437a_1525),
        (83_987, 0x8d5a_b0b8_68dc_4c37_0e15_804c_b3f0_d063),
        (89_568, 0x9441_2526_87a7_8cd4_7c42_18bc_f0fb_c38b),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (119_259, 0x43a1_381d_556b_fb78_f7c3_ac07_a826_8b2a),
        (71_251, 0x1375_b6f2_a58c_e31c_355c_5bd7_54d1_c2bf),
        (73_662, 0x03a1_c05f_df4a_c1ea_60a9_8d8a_742e_0664),
        (79_243, 0x79ed_3bd6_553f_91d7_484a_889f_d994_a5d4),
        (85_381, 0xcd30_8033_fd36_3841_67a7_afb6_19a8_6526),
        (90_962, 0xbcce_7403_63cc_d3a4_8c6c_6940_4e26_5e36),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (117_931, 0x76f5_8a3e_92a9_e8c3_3933_b0a5_0a7e_4558),
        (69_923, 0x2642_01f5_dc77_ff1f_8289_127a_7ea4_32d7),
        (72_334, 0x4619_a8c2_6034_bf6d_769c_7499_19b3_f930),
        (77_915, 0xf5b1_0552_52f9_a7f8_928c_a9bd_98e0_f870),
        (84_053, 0x436c_4382_70fc_ce11_f0ae_a4fc_fc03_0596),
        (89_634, 0x3821_875a_a107_e8d6_ded9_0bdd_40f1_d03e),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (119_325, 0x6359_127e_b180_30fc_7a29_95f8_1bcd_3f75),
        (71_317, 0x2de6_0c24_f2c0_797c_061e_4f84_cb2a_37ea),
        (73_728, 0xef6c_9df1_21bd_7a52_2dad_6066_b15c_e9f1),
        (79_309, 0xf471_002f_725b_2c1a_bb30_1d3e_59c6_b301),
        (85_447, 0xabd4_a4ce_7a76_53b0_19ae_41ff_a234_a153),
        (91_028, 0x23f7_a89b_a004_1c94_0693_e085_b682_5353),
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
fn final_forward_assemblies_match_invariant_baseline_fingerprints() {
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

// Unpaired quads use non-invariant positions and the same zero-bias receiver definition.
const UNLIT_QUAD_SOURCES: &[(usize, u128)] = &[
    (93_471, 0x5509_e28c_a0cc_bfd1_606a_bd83_5ba0_9dd0),
    (93_890, 0x4743_4933_7d2b_c547_5ee9_495f_b153_47b4),
    (95_284, 0x7e21_a164_40f9_ca84_72f2_91e2_e919_942f),
    (97_505, 0xde2f_7433_7bf2_f29a_dab5_0e18_3c04_d27e),
    (97_924, 0xd2f3_4467_a6ba_b35e_c98c_588e_1ff2_3d2e),
    (99_318, 0x7064_ac3b_354e_6abb_22df_d65e_1b71_af25),
    (109_984, 0x13b8_59c6_3acf_0d10_4e86_cd89_bd79_92a9),
    (114_018, 0x9e82_0966_e518_19f8_ecf0_9bf4_06f0_fc4f),
    (113_506, 0xe946_8b86_0dd8_2613_bcc4_29d4_50a0_9da3),
    (114_900, 0xf176_4cc3_0846_bae8_812b_b43f_70d1_7214),
    (117_854, 0xbf2d_879d_5ee7_b57f_145a_f97d_5668_490d),
    (119_248, 0x60d4_b0b8_88e9_e7a8_1329_30ec_ab86_5254),
    (110_403, 0x0eb3_0901_363e_78ca_d688_eb27_56dc_74dd),
    (111_797, 0xe3ae_4c0b_647c_61ab_2953_e093_d232_8af6),
    (114_437, 0xdd91_558f_6039_2bf4_0b35_058f_be83_0acf),
    (115_831, 0x2809_c4c2_e104_935b_27bf_42fd_341d_08b4),
    (113_506, 0xe946_8b86_0dd8_2613_bcc4_29d4_50a0_9da3),
    (114_900, 0xf176_4cc3_0846_bae8_812b_b43f_70d1_7214),
    (117_854, 0xbf2d_879d_5ee7_b57f_145a_f97d_5668_490d),
    (119_248, 0x60d4_b0b8_88e9_e7a8_1329_30ec_ab86_5254),
    (96_993, 0x00a5_ee5b_9db2_a532_29c8_49b1_e3d3_ad7a),
    (98_387, 0x96c4_da16_ac82_20e4_528e_8688_8e92_4451),
    (97_059, 0xd587_eedf_51b3_d942_8eed_45f1_6289_406b),
    (98_453, 0x21c9_a714_8c85_e495_3807_8090_4834_9154),
    (101_341, 0x3431_f75c_619f_ada3_23b4_b908_840e_f5c8),
    (102_735, 0xf231_b2a0_6253_2ee4_9644_4489_49b8_0651),
    (101_407, 0x10cb_3513_a9cb_a365_45c3_c0d1_9b49_e6b7),
    (102_801, 0x17e2_a4b9_ddbc_4345_552d_293c_c3c7_7f3a),
    (113_506, 0xe946_8b86_0dd8_2613_bcc4_29d4_50a0_9da3),
    (114_900, 0xf176_4cc3_0846_bae8_812b_b43f_70d1_7214),
    (113_572, 0x3dc0_6f93_e960_a549_8bc0_0ab6_0d7a_10e6),
    (114_966, 0x1b21_667a_2798_070e_b759_21e0_9f4b_f3ad),
    (117_854, 0xbf2d_879d_5ee7_b57f_145a_f97d_5668_490d),
    (119_248, 0x60d4_b0b8_88e9_e7a8_1329_30ec_ab86_5254),
    (117_920, 0x3401_2e5b_86ce_39b1_f8e7_04df_1e32_4c0a),
    (119_314, 0x0663_e893_3678_85d2_e587_64d9_bb96_a4cf),
];

#[test]
fn unlit_quad_assemblies_pin_the_zero_bias_receiver_definition() {
    let cases = final_forward_sources();
    assert_eq!(cases.len(), UNLIT_QUAD_SOURCES.len());
    for ((label, sources), expected) in cases.into_iter().zip(UNLIT_QUAD_SOURCES) {
        assert_eq!(
            super::source_fingerprint(&crate::renderer::unlit_source(&sources[0])),
            *expected,
            "{label} unpaired quad source"
        );
    }
}
