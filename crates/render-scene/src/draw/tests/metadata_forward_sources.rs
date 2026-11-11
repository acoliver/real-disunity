//! Complete forward source assemblies, including their final screen-space and terrain rewrites.

// Expected byte lengths and FNV-1a/128 from production assembly with invariant positions.
// Each row is object, terrain, then conventional/cached/layered/cached-layered terrain.
const FINAL_FORWARD_SOURCES: &[[(usize, u128); 6]] = &[
    // ordinary velocity=false ssao=false receiver=Nearest
    [
        (93_482, 0x22e8_31dc_934f_cb24_2e4f_660c_23cb_6188),
        (51_021, 0x5aee_0909_1af2_a2f6_1004_61bd_40ec_af61),
        (53_432, 0xf413_872d_a16e_f879_1010_9fcf_19cf_8d32),
        (59_013, 0x1fb2_b08d_6503_755d_81f4_f957_e5d9_fc16),
        (65_151, 0x769a_ae40_4a3e_8e58_78a4_b4e9_4942_ded0),
        (70_732, 0x440a_6307_e789_fd20_13f8_d712_a498_60ac),
    ],
    // ordinary velocity=false ssao=true receiver=Nearest
    [
        (93_901, 0xa129_c0de_4c4b_74bd_fd3f_4320_98ae_81aa),
        (51_440, 0x87dc_7f9f_c6c5_be64_cb4e_221a_1148_b02b),
        (53_851, 0xc555_6a53_8eba_126e_c6c8_a657_abeb_898e),
        (59_432, 0x89db_cb6e_989d_4d99_d4a0_9844_d279_adc8),
        (65_570, 0x3ba2_b16e_6e03_7044_0fd4_6d5f_1ac3_cf2e),
        (71_151, 0xa7e4_3f7c_5e0e_c63c_e18b_d854_459c_f108),
    ],
    // ordinary velocity=false ssao=true receiver=DepthAwareGather
    [
        (95_295, 0xc6b6_7746_5a6b_38d0_0960_4c28_6184_1701),
        (52_834, 0x6187_e2c1_08b6_73aa_0b53_f5d1_29fb_8fb0),
        (55_245, 0xa60d_29f8_b9fc_8678_20cf_1152_0bc2_de89),
        (60_826, 0x587c_8a88_6e3b_d745_2120_0ddd_bfd1_9e0b),
        (66_964, 0x6dd4_9c23_1832_b444_7258_78f4_57ce_0059),
        (72_545, 0xf13e_1073_c5cc_9d49_0116_196d_fdeb_26c3),
    ],
    // ordinary velocity=true ssao=false receiver=Nearest
    [
        (97_516, 0x9d48_4777_7456_6105_da55_1b14_468e_f896),
        (51_384, 0x607e_189c_5f5a_db00_2e3f_7cfc_470a_0ba3),
        (53_795, 0x4428_2e5f_4f1b_5d89_e808_f02a_9774_38ea),
        (59_376, 0xdd6e_5374_852b_2de2_55cb_f67f_2417_89b2),
        (65_514, 0x311f_2b15_cc87_2da4_c8a1_14bd_21d8_d774),
        (71_095, 0x4207_f80e_da5e_6079_9079_edd7_83d1_ca44),
    ],
    // ordinary velocity=true ssao=true receiver=Nearest
    [
        (97_935, 0x4ec9_638b_7def_5d2e_5ae9_5295_2cc4_8504),
        (51_803, 0x0c3c_957c_24c1_5e4e_b1b2_ce6f_ab67_cbc3),
        (54_214, 0xe46f_cd19_7cde_e9c9_5af3_7572_efc9_09ac),
        (59_795, 0x5af0_bb2f_e563_e9ed_b9c9_355e_775b_f472),
        (65_933, 0x84de_6269_680c_b8e3_7dad_97a8_7d03_f58c),
        (71_514, 0xd44f_7d4e_2ea4_7e55_82ba_f7ae_35a0_fa42),
    ],
    // ordinary velocity=true ssao=true receiver=DepthAwareGather
    [
        (99_329, 0xcc1b_19f0_6626_aa1f_3046_95ce_99a1_96bf),
        (53_197, 0xb043_4e00_8f5a_0f98_3c17_fa59_1a43_41e0),
        (55_608, 0x16fc_d4bd_01e3_cc7d_9658_662d_ea88_8333),
        (61_189, 0xb567_8782_bfae_1501_33bd_b245_17c4_1519),
        (67_327, 0x4fdb_0079_7cb1_878f_2a5b_641d_8758_11fb),
        (72_908, 0x2f2e_7838_a2fb_4ce0_5e24_710a_7b3f_efe9),
    ],
    // receiver ssao=false reflection=false velocity=false receiver=Nearest
    [
        (109_371, 0xb4d0_a58f_16bb_82c2_bfcd_6115_674f_bc2b),
        (66_910, 0xbd8d_3a4a_24dd_031e_115a_6484_d6ec_a69c),
        (69_321, 0xd1aa_7b71_f967_69f5_518f_a4c2_2c92_c979),
        (74_902, 0x5af1_d395_2307_0e60_c48c_a402_e78a_da83),
        (81_040, 0x6f97_2e6c_0f38_4603_cb11_d607_ca3e_4f79),
        (86_621, 0xf23b_3630_5151_1752_3740_b87c_f535_5ddb),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (113_405, 0x5eff_7820_df4a_a39c_4f5a_a086_3ae8_86e1),
        (67_273, 0xc62a_cdd9_7052_e24d_0091_89a2_45f9_bd20),
        (69_684, 0x20b7_c014_d898_5850_ae91_8071_daec_4abb),
        (75_265, 0x0fb2_af6e_e134_c9b4_9d4c_bc42_1788_3969),
        (81_403, 0x54c7_1ab1_cf33_22c0_7bc4_2fd8_5fd8_2c03),
        (86_984, 0xaa7f_67a5_9c09_fe5d_cba0_5bfc_cf7c_f119),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (69_374, 0xb699_3aab_5894_4654_1615_03d5_c3c5_933d),
        (71_785, 0x9e0d_6f98_afe5_ec1d_d098_6020_a106_6b3c),
        (77_366, 0xa3ba_8595_3c6b_6838_d83a_9495_5c78_8aa8),
        (83_504, 0xfbaf_db79_cf3a_7a63_713a_678b_fe9a_f11a),
        (89_085, 0x6c6c_0bac_9e14_34b9_4805_25ad_16d5_0ab6),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (70_768, 0x9f3d_3887_57cf_de92_415a_fe36_cd0a_2aa6),
        (73_179, 0xdcba_657a_f97d_2af6_0341_29c1_b511_9333),
        (78_760, 0xe325_3291_645a_7f7b_b674_0167_5577_61d3),
        (84_898, 0xd0c9_d3fd_a683_a789_0c4d_5fed_fea7_0465),
        (90_479, 0x3b82_e95e_f0d8_b9af_c22d_803f_2992_f675),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (69_731, 0xb626_db58_9889_4c3e_aa52_c8b9_6e74_e56e),
        (72_142, 0xcb0f_39f1_c6d0_a787_dc5d_01b0_0573_9cc1),
        (77_723, 0x604b_c72c_66a7_5728_75c7_245f_45f0_5665),
        (83_861, 0xcd6c_7353_0be5_b737_c67e_bc3e_b348_4727),
        (89_442, 0x2cbb_fead_a0ed_ee85_e8f9_0800_0697_bad3),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (71_125, 0x45ab_5b13_fa27_089b_4ee8_8b9c_7367_d97b),
        (73_536, 0x96bd_8fa3_0c82_3126_2a1b_a7b8_47f5_cfc0),
        (79_117, 0x9370_5111_0254_e908_aaa4_c0e0_a41a_ce74),
        (85_255, 0xca82_eaaa_f0cd_347f_cb5c_cffb_7d57_04da),
        (90_836, 0x4a5a_51e6_5120_45b0_b9a7_c4c0_b3d3_087e),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (109_790, 0xd613_0cb8_e05d_19f2_45d1_09f3_8749_f19d),
        (67_329, 0xb58a_d3c4_bd38_f53e_996f_4cda_3ff8_5e82),
        (69_740, 0x698b_1f38_d413_68cd_aa73_51fd_3262_594d),
        (75_321, 0xb454_badc_3754_0792_96ce_e21d_ff32_11c9),
        (81_459, 0x62a7_dc8f_31cd_6595_4e35_c6cc_4b98_0667),
        (87_040, 0x2e90_3ec1_dd12_67d4_5611_1504_bc94_630b),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (111_184, 0xa087_2c85_d593_f066_f476_7596_0601_33ee),
        (68_723, 0xfc5a_12b8_1459_abb3_36bd_2715_ac8a_c859),
        (71_134, 0x9817_b301_620e_8473_279d_7c84_8600_3d3a),
        (76_715, 0x5564_0175_5479_cb8a_018d_9bb4_d3a3_24ea),
        (82_853, 0x8f7f_2e34_ba5f_7e4d_4753_9bca_f14e_5780),
        (88_434, 0xa99a_d7e5_3711_2590_5d41_8516_8de8_01c8),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (113_824, 0x2f5d_59db_d9ee_2792_00bc_8f83_9a8d_905f),
        (67_692, 0x8ad0_bac4_9d38_0d29_385b_f393_8679_6a28),
        (70_103, 0x112b_79c8_82f2_36d9_6ce3_96b9_1049_fa89),
        (75_684, 0x8fc1_e530_47fe_4b38_06b2_fe77_cd68_b159),
        (81_822, 0x45e1_7db4_a836_bf76_871c_aa5d_149c_0eff),
        (87_403, 0x798d_3324_3b28_497c_ca4c_86d5_2171_a697),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (115_218, 0x0da6_b367_dada_3d1c_0144_084a_fda4_200c),
        (69_086, 0xf321_1a01_219b_e3b5_3775_b6fc_2782_34c3),
        (71_497, 0x59db_a26a_d8b8_9d85_cd0b_ca97_000d_a4c6),
        (77_078, 0xb2ab_ff51_fbf4_8a82_a0c3_6bf5_60ce_3a3a),
        (83_216, 0x0632_398e_49fb_40e0_7b22_910b_6819_1ca8),
        (88_797, 0x63d7_482f_a01a_f642_9063_aab3_e143_87b4),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (69_374, 0xb699_3aab_5894_4654_1615_03d5_c3c5_933d),
        (71_785, 0x9e0d_6f98_afe5_ec1d_d098_6020_a106_6b3c),
        (77_366, 0xa3ba_8595_3c6b_6838_d83a_9495_5c78_8aa8),
        (83_504, 0xfbaf_db79_cf3a_7a63_713a_678b_fe9a_f11a),
        (89_085, 0x6c6c_0bac_9e14_34b9_4805_25ad_16d5_0ab6),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (70_768, 0x9f3d_3887_57cf_de92_415a_fe36_cd0a_2aa6),
        (73_179, 0xdcba_657a_f97d_2af6_0341_29c1_b511_9333),
        (78_760, 0xe325_3291_645a_7f7b_b674_0167_5577_61d3),
        (84_898, 0xd0c9_d3fd_a683_a789_0c4d_5fed_fea7_0465),
        (90_479, 0x3b82_e95e_f0d8_b9af_c22d_803f_2992_f675),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (69_731, 0xb626_db58_9889_4c3e_aa52_c8b9_6e74_e56e),
        (72_142, 0xcb0f_39f1_c6d0_a787_dc5d_01b0_0573_9cc1),
        (77_723, 0x604b_c72c_66a7_5728_75c7_245f_45f0_5665),
        (83_861, 0xcd6c_7353_0be5_b737_c67e_bc3e_b348_4727),
        (89_442, 0x2cbb_fead_a0ed_ee85_e8f9_0800_0697_bad3),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (71_125, 0x45ab_5b13_fa27_089b_4ee8_8b9c_7367_d97b),
        (73_536, 0x96bd_8fa3_0c82_3126_2a1b_a7b8_47f5_cfc0),
        (79_117, 0x9370_5111_0254_e908_aaa4_c0e0_a41a_ce74),
        (85_255, 0xca82_eaaa_f0cd_347f_cb5c_cffb_7d57_04da),
        (90_836, 0x4a5a_51e6_5120_45b0_b9a7_c4c0_b3d3_087e),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=Nearest
    [
        (97_004, 0x3671_0718_ad4d_29a2_d276_4bf9_3b21_4892),
        (53_485, 0x2560_7c2c_20c0_0715_6819_5a36_71f9_7866),
        (55_896, 0x856a_352b_e4dc_0146_696d_d27c_5187_0cc9),
        (61_477, 0x3350_97c2_765b_04e3_82b1_1c9c_96d1_d9b3),
        (67_615, 0xe398_2338_9db5_d931_9232_03e5_d717_fd11),
        (73_196, 0x19a6_0f03_de8b_27d1_48fe_6bdb_6a3c_7073),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=DepthAwareGather
    [
        (98_398, 0xac9d_7f89_f8e9_96f6_ba25_9258_1f99_34bd),
        (54_879, 0xd2f9_3844_5aaf_4f31_8db9_d6eb_4b3c_15d5),
        (57_290, 0x3f5a_de72_9353_f094_4b72_6ea4_ee3c_ce2e),
        (62_871, 0xeee7_8095_b2f2_ec17_4433_03df_1ffa_d358),
        (69_009, 0xbdb7_0c15_dd69_2748_714d_3a74_324d_5476),
        (74_590, 0x4d38_8f68_6e51_c3b9_96e0_8813_6675_7650),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=Nearest
    [
        (97_070, 0xca78_e2a8_df73_c438_8480_7ec0_2496_7bb3),
        (53_551, 0x1bef_0d3e_9ed6_90c3_ecc6_95ce_9afd_b1c7),
        (55_962, 0xf3ff_251d_2d28_b94c_5599_5887_e128_4188),
        (61_543, 0x3fcd_324c_748a_812c_d0b2_f310_fa07_c372),
        (67_681, 0x33f2_32a5_b6ba_34bc_e696_001b_f8bf_7c50),
        (73_262, 0x30c9_254d_4b07_2d48_eb71_0e47_76f2_1f4a),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=DepthAwareGather
    [
        (98_464, 0x000a_3df6_a182_a474_92d1_2c99_2c51_6e38),
        (54_945, 0x6098_8ee2_f40f_577a_ec7e_dd43_1901_f46c),
        (57_356, 0x5774_2826_0f00_eceb_95fb_90bc_1af8_0e0f),
        (62_937, 0xf950_b356_d04e_9876_2cac_aa69_64ca_46b1),
        (69_075, 0x7d17_e023_7e57_6187_26d2_0231_bbbf_5c4f),
        (74_656, 0x25c0_7d0c_344a_43d8_fa29_957e_651c_2db9),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=Nearest
    [
        (101_352, 0x1152_d7d5_378c_be46_e0f8_5f81_b8a5_1560),
        (53_842, 0xce2e_481f_a2a0_259f_7187_b14d_9e1c_d4a3),
        (56_253, 0x2c8d_c151_016f_6ce1_9422_1f62_2666_c70a),
        (61_834, 0xa1a0_d7d9_9386_7d44_1656_d9ce_ea87_8678),
        (67_972, 0x6f1a_cd0c_df37_aad2_ff9c_c7d8_9658_ad12),
        (73_553, 0xb832_8601_e7b0_a802_9954_272d_ea9d_eca0),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=DepthAwareGather
    [
        (102_746, 0x80bd_7abc_fdfc_1da7_18d1_78c5_6910_8715),
        (55_236, 0x0474_da9c_d8de_7b76_2d49_06c2_e2e0_f7aa),
        (57_647, 0x9fe5_091b_f42c_57ec_453c_0a52_b436_b483),
        (63_228, 0xb545_ce30_a997_b028_05ba_044e_9d93_61fd),
        (69_366, 0xe4ad_2668_3ba4_1680_c2c2_73df_cc2b_a37b),
        (74_947, 0xa38e_7832_3791_545f_6d0f_83aa_f3b9_1efd),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=Nearest
    [
        (101_418, 0x241a_48d2_1a34_1ad4_e28c_b428_00fb_e4bf),
        (53_908, 0x1e7b_a9c9_1e3b_eb55_6c77_35da_b66d_e8de),
        (56_319, 0x6ec8_66cc_0ab1_05dc_e2ed_68c5_0aef_93cf),
        (61_900, 0xf0b9_8212_0f2b_fe15_988d_08e6_14cd_0a05),
        (68_038, 0x2da5_d317_d83a_dffe_3800_2041_8c4e_da5f),
        (73_619, 0x335d_aecd_b87d_e8e9_744b_82c4_f1ec_373d),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=DepthAwareGather
    [
        (102_812, 0x0cea_ed31_b205_5d15_7730_a02f_9918_8166),
        (55_302, 0x13e1_ba4c_0761_1a31_4767_2d83_8089_56e7),
        (57_713, 0xfe1d_3739_d666_fde3_0d87_a166_32b0_d4d6),
        (63_294, 0x6821_9ca1_2e3e_99a9_c7bd_3a96_993e_5a80),
        (69_432, 0xf37a_8682_ccbb_84f5_7933_5e28_a498_0976),
        (75_013, 0xc1bf_b72a_fc4c_704e_dd0f_6520_da67_f430),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=Nearest
    [
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (69_374, 0xb699_3aab_5894_4654_1615_03d5_c3c5_933d),
        (71_785, 0x9e0d_6f98_afe5_ec1d_d098_6020_a106_6b3c),
        (77_366, 0xa3ba_8595_3c6b_6838_d83a_9495_5c78_8aa8),
        (83_504, 0xfbaf_db79_cf3a_7a63_713a_678b_fe9a_f11a),
        (89_085, 0x6c6c_0bac_9e14_34b9_4805_25ad_16d5_0ab6),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (70_768, 0x9f3d_3887_57cf_de92_415a_fe36_cd0a_2aa6),
        (73_179, 0xdcba_657a_f97d_2af6_0341_29c1_b511_9333),
        (78_760, 0xe325_3291_645a_7f7b_b674_0167_5577_61d3),
        (84_898, 0xd0c9_d3fd_a683_a789_0c4d_5fed_fea7_0465),
        (90_479, 0x3b82_e95e_f0d8_b9af_c22d_803f_2992_f675),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (112_959, 0xd6ae_f592_dd4c_3a55_f464_498d_1a6d_ba68),
        (69_440, 0xefd0_942d_8d14_2ee8_6cf6_5f60_6beb_c3d4),
        (71_851, 0xb6a7_8865_0840_dc9d_9c00_222a_24ed_630d),
        (77_432, 0xc034_4134_2d8d_ffe1_70be_0440_3d14_1a91),
        (83_570, 0xeef9_e359_7a42_5343_282e_ab4f_4174_361b),
        (89_151, 0x1b41_796f_c764_5fb3_df94_8baa_3281_dcbf),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (114_353, 0x5f06_5d1f_a862_1bb2_ef37_47cc_a6c3_4b4f),
        (70_834, 0x7a0b_61a5_71bd_82d8_059c_1704_414e_89f7),
        (73_245, 0x6ed2_d234_bdfb_ceb3_7fa6_d13e_0990_81f2),
        (78_826, 0x57e3_278b_1dc2_8c39_9ed8_505d_f5e7_84d2),
        (84_964, 0x36b1_af3e_459e_21ac_b76f_cf11_28e7_202c),
        (90_545, 0x6097_0005_bc89_7882_b013_c89c_da5b_991c),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (69_731, 0xb626_db58_9889_4c3e_aa52_c8b9_6e74_e56e),
        (72_142, 0xcb0f_39f1_c6d0_a787_dc5d_01b0_0573_9cc1),
        (77_723, 0x604b_c72c_66a7_5728_75c7_245f_45f0_5665),
        (83_861, 0xcd6c_7353_0be5_b737_c67e_bc3e_b348_4727),
        (89_442, 0x2cbb_fead_a0ed_ee85_e8f9_0800_0697_bad3),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (71_125, 0x45ab_5b13_fa27_089b_4ee8_8b9c_7367_d97b),
        (73_536, 0x96bd_8fa3_0c82_3126_2a1b_a7b8_47f5_cfc0),
        (79_117, 0x9370_5111_0254_e908_aaa4_c0e0_a41a_ce74),
        (85_255, 0xca82_eaaa_f0cd_347f_cb5c_cffb_7d57_04da),
        (90_836, 0x4a5a_51e6_5120_45b0_b9a7_c4c0_b3d3_087e),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (117_307, 0xa6b8_60f3_a03a_0be9_c128_b751_e72e_5b8c),
        (69_797, 0x82ec_4541_a40f_4dd7_9a0d_ebe1_4e69_5e2b),
        (72_208, 0xc64d_363b_88f6_042e_48a1_6f0c_1921_6ef4),
        (77_789, 0x9868_2a89_cb01_31bc_5781_4a73_5987_d230),
        (83_927, 0x7178_b9a6_f36c_d770_87be_218b_02e4_dfb2),
        (89_508, 0x9064_daed_2a29_26e3_9cc2_b1da_0895_7ff6),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (118_701, 0x263a_0b8b_cc5e_bf0d_7e60_79e8_d348_4be9),
        (71_191, 0x9a80_f15a_0c8d_22f9_b5c8_22b1_2a50_001e),
        (73_602, 0xfc7d_1d32_5961_4573_aeaf_a23b_2c91_8b25),
        (79_183, 0xdbdb_99d2_1e04_6301_e43f_910d_2bf2_5a21),
        (85_321, 0xcb39_533b_6cd0_e439_b1d5_74ad_1055_67bf),
        (90_902, 0x51b5_8801_8310_7515_c83c_95ee_bcf1_04eb),
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
                crate::terrain::program::source::test_sources(&terrain);
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

// Unpaired quad assemblies keep main's non-invariant position compilation.
const UNLIT_QUAD_SOURCES: &[(usize, u128)] = &[
    (93_471, 0x5509_e28c_a0cc_bfd1_606a_bd83_5ba0_9dd0),
    (93_890, 0x4743_4933_7d2b_c547_5ee9_495f_b153_47b4),
    (95_284, 0x7e21_a164_40f9_ca84_72f2_91e2_e919_942f),
    (97_505, 0xde2f_7433_7bf2_f29a_dab5_0e18_3c04_d27e),
    (97_924, 0xd2f3_4467_a6ba_b35e_c98c_588e_1ff2_3d2e),
    (99_318, 0x7064_ac3b_354e_6abb_22df_d65e_1b71_af25),
    (109_360, 0x4325_347a_8d8d_8fa5_bbf8_c834_869c_e82d),
    (113_394, 0xfd80_7300_ce2c_e916_cb76_ed06_791c_b8ab),
    (112_882, 0xadf5_f5f9_f087_6acd_4748_c762_881f_3ebf),
    (114_276, 0x8c4a_37f0_1fe2_a223_3399_2259_249f_9ac0),
    (117_230, 0xf048_1556_35ee_1aca_c78f_9c04_ad81_9f91),
    (118_624, 0xad4b_462b_bae4_daeb_a5fe_8edd_e10f_1e68),
    (109_779, 0x396a_55c5_516e_0f29_72bb_1cef_3dca_75a5),
    (111_173, 0x7f64_7438_6325_92e2_d912_74ab_f0a6_8dae),
    (113_813, 0x9208_16d4_68bf_42fb_6d80_cf40_4479_a247),
    (115_207, 0xa35b_d99e_5c9e_1bf6_5b3a_ccca_4e7f_9dfc),
    (112_882, 0xadf5_f5f9_f087_6acd_4748_c762_881f_3ebf),
    (114_276, 0x8c4a_37f0_1fe2_a223_3399_2259_249f_9ac0),
    (117_230, 0xf048_1556_35ee_1aca_c78f_9c04_ad81_9f91),
    (118_624, 0xad4b_462b_bae4_daeb_a5fe_8edd_e10f_1e68),
    (96_993, 0x00a5_ee5b_9db2_a532_29c8_49b1_e3d3_ad7a),
    (98_387, 0x96c4_da16_ac82_20e4_528e_8688_8e92_4451),
    (97_059, 0xd587_eedf_51b3_d942_8eed_45f1_6289_406b),
    (98_453, 0x21c9_a714_8c85_e495_3807_8090_4834_9154),
    (101_341, 0x3431_f75c_619f_ada3_23b4_b908_840e_f5c8),
    (102_735, 0xf231_b2a0_6253_2ee4_9644_4489_49b8_0651),
    (101_407, 0x10cb_3513_a9cb_a365_45c3_c0d1_9b49_e6b7),
    (102_801, 0x17e2_a4b9_ddbc_4345_552d_293c_c3c7_7f3a),
    (112_882, 0xadf5_f5f9_f087_6acd_4748_c762_881f_3ebf),
    (114_276, 0x8c4a_37f0_1fe2_a223_3399_2259_249f_9ac0),
    (112_948, 0xf8ec_bff6_d43a_bd83_1ce7_8343_e604_ae6a),
    (114_342, 0x6442_e17d_67f3_928d_1e22_9041_a6a2_2b21),
    (117_230, 0xf048_1556_35ee_1aca_c78f_9c04_ad81_9f91),
    (118_624, 0xad4b_462b_bae4_daeb_a5fe_8edd_e10f_1e68),
    (117_296, 0x905e_041d_5b0d_d5f2_ecbc_7df8_d100_a3b6),
    (118_690, 0x9091_e44f_6c8c_637c_bef0_bddd_75d5_fd5b),
];

#[test]
fn unlit_quad_assemblies_preserve_main_fingerprints() {
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
