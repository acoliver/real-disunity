//! Complete forward source assemblies, including their final screen-space and terrain rewrites.

// Expected byte lengths and FNV-1a/128 from production assembly with invariant positions.
// Shadowed sources include exact zero-bias position and integer-selected derivative bits.
// Each row is object, terrain, then conventional/cached/layered/cached-layered terrain.
const FINAL_FORWARD_SOURCES: &[[(usize, u128); 6]] = &[
    // ordinary velocity=false ssao=false receiver=Nearest
    [
        (95_438, 0x426c_8723_c9fb_4291_8336_e4d6_8445_36aa),
        (51_021, 0x5aee_0909_1af2_a2f6_1004_61bd_40ec_af61),
        (53_432, 0xf413_872d_a16e_f879_1010_9fcf_19cf_8d32),
        (59_013, 0x1fb2_b08d_6503_755d_81f4_f957_e5d9_fc16),
        (65_151, 0x769a_ae40_4a3e_8e58_78a4_b4e9_4942_ded0),
        (70_732, 0x440a_6307_e789_fd20_13f8_d712_a498_60ac),
    ],
    // ordinary velocity=false ssao=true receiver=Nearest
    [
        (95_857, 0xbaa7_1b63_63b9_c0ee_2df6_83a5_f4cc_8514),
        (51_440, 0x87dc_7f9f_c6c5_be64_cb4e_221a_1148_b02b),
        (53_851, 0xc555_6a53_8eba_126e_c6c8_a657_abeb_898e),
        (59_432, 0x89db_cb6e_989d_4d99_d4a0_9844_d279_adc8),
        (65_570, 0x3ba2_b16e_6e03_7044_0fd4_6d5f_1ac3_cf2e),
        (71_151, 0xa7e4_3f7c_5e0e_c63c_e18b_d854_459c_f108),
    ],
    // ordinary velocity=false ssao=true receiver=DepthAwareGather
    [
        (97_251, 0xbec6_72a8_fbf5_8304_17a5_77d6_521f_3eb7),
        (52_834, 0x6187_e2c1_08b6_73aa_0b53_f5d1_29fb_8fb0),
        (55_245, 0xa60d_29f8_b9fc_8678_20cf_1152_0bc2_de89),
        (60_826, 0x587c_8a88_6e3b_d745_2120_0ddd_bfd1_9e0b),
        (66_964, 0x6dd4_9c23_1832_b444_7258_78f4_57ce_0059),
        (72_545, 0xf13e_1073_c5cc_9d49_0116_196d_fdeb_26c3),
    ],
    // ordinary velocity=true ssao=false receiver=Nearest
    [
        (99_472, 0xa268_dcad_e45f_0729_66b8_7789_ecc3_f260),
        (51_384, 0x607e_189c_5f5a_db00_2e3f_7cfc_470a_0ba3),
        (53_795, 0x4428_2e5f_4f1b_5d89_e808_f02a_9774_38ea),
        (59_376, 0xdd6e_5374_852b_2de2_55cb_f67f_2417_89b2),
        (65_514, 0x311f_2b15_cc87_2da4_c8a1_14bd_21d8_d774),
        (71_095, 0x4207_f80e_da5e_6079_9079_edd7_83d1_ca44),
    ],
    // ordinary velocity=true ssao=true receiver=Nearest
    [
        (99_891, 0x2051_7a0b_ee67_b6d1_af29_12f6_434f_966e),
        (51_803, 0x0c3c_957c_24c1_5e4e_b1b2_ce6f_ab67_cbc3),
        (54_214, 0xe46f_cd19_7cde_e9c9_5af3_7572_efc9_09ac),
        (59_795, 0x5af0_bb2f_e563_e9ed_b9c9_355e_775b_f472),
        (65_933, 0x84de_6269_680c_b8e3_7dad_97a8_7d03_f58c),
        (71_514, 0xd44f_7d4e_2ea4_7e55_82ba_f7ae_35a0_fa42),
    ],
    // ordinary velocity=true ssao=true receiver=DepthAwareGather
    [
        (101_285, 0x0bef_b4e6_84ef_db36_3c0a_9fa4_464a_e86d),
        (53_197, 0xb043_4e00_8f5a_0f98_3c17_fa59_1a43_41e0),
        (55_608, 0x16fc_d4bd_01e3_cc7d_9658_662d_ea88_8333),
        (61_189, 0xb567_8782_bfae_1501_33bd_b245_17c4_1519),
        (67_327, 0x4fdb_0079_7cb1_878f_2a5b_641d_8758_11fb),
        (72_908, 0x2f2e_7838_a2fb_4ce0_5e24_710a_7b3f_efe9),
    ],
    // receiver ssao=false reflection=false velocity=false receiver=Nearest
    [
        (111_951, 0x9b08_f164_48af_3cdc_4757_d755_f47d_e47d),
        (69_077, 0x0182_2e98_c9cb_a155_3d10_803e_d7f9_3b46),
        (71_488, 0xf8c0_1af6_00cc_ab8a_4c1c_467e_8369_91fd),
        (77_069, 0xf0a5_7c93_0aea_8468_befc_c815_f755_42bb),
        (83_207, 0x0489_7d5f_4c3f_4eb3_422f_f4f1_e91d_9d5d),
        (88_788, 0x0a2e_ed02_f996_ef22_3d7c_bc76_a1b6_f8f3),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (115_985, 0x3707_f16f_d6c9_e948_5ec7_160a_56a3_fc63),
        (69_440, 0x4779_889a_3b10_8288_ab87_a08a_d713_83fa),
        (71_851, 0xaf84_c09c_7bff_ef5a_6776_ab0a_445e_67a7),
        (77_432, 0x2bb4_5639_d3ed_6d71_5af8_f0e5_a53a_2be1),
        (83_570, 0x1928_e771_e68e_08b4_c9a2_9c08_dacb_194f),
        (89_151, 0x56f8_5738_e738_3c16_b0f0_2fe2_92e2_acf1),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (115_473, 0xd7a3_5255_3b94_fa13_8cbc_0096_1a0e_c0af),
        (71_541, 0xb331_1796_e11b_b2c0_73eb_7d81_b254_db31),
        (73_952, 0x758b_00e3_f010_3163_393f_ca28_99e1_a832),
        (79_533, 0xf4a0_25ad_2119_0f04_cebb_27e8_70dd_d396),
        (85_671, 0xfd36_0bba_17b9_7725_af76_1b49_c9f6_d7a0),
        (91_252, 0x379a_72a5_0488_dae8_9b4f_28f1_dbe8_fedc),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (116_867, 0x360c_e8d3_fe99_45eb_9b3f_91d9_34ea_a0e4),
        (72_935, 0x27ee_86da_aecd_5fd3_20c2_73d0_803d_ebda),
        (75_346, 0x8e74_d4ae_f564_de2f_62e4_59ed_fd8b_b3fd),
        (80_927, 0x42a4_c9cd_e8c3_7e45_5989_91ea_10c0_5285),
        (87_065, 0x8d49_28a5_0013_5803_2fb5_209c_a756_320f),
        (92_646, 0x528d_a3ba_4666_a5a6_7efa_03ca_d4e5_161f),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (119_821, 0x439b_406d_c875_ce53_2c67_8c8e_71ef_1911),
        (71_898, 0x8680_d629_9c55_a78d_56ae_b6b3_1428_25f2),
        (74_309, 0x4ea9_da6f_28f0_0f49_be13_0928_bdaa_be8f),
        (79_890, 0x0a48_ad8f_7ab3_2de8_d323_83f9_18f3_6f93),
        (86_028, 0x016c_c4dd_8d44_91ab_ed7c_a8a0_5307_dc9d),
        (91_609, 0x16e7_9e68_a883_cca9_7028_8451_421e_e321),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (121_215, 0x36b2_4ec9_ea1d_e1b4_55bb_763e_6553_49dc),
        (73_292, 0xeef3_3d9e_39a4_55e8_ed85_124c_27b1_5e1f),
        (75_703, 0xae6e_8671_cf04_abd0_5409_7686_a110_2bf2),
        (81_284, 0x46f8_6569_2dd6_b2c5_ec31_c97d_d414_256e),
        (87_422, 0x53e0_4cf4_4522_6fbf_e377_5f38_200d_5dc4),
        (93_003, 0xfe62_da79_0cfd_c805_41a0_d18e_070d_f2d8),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (112_370, 0x5be3_21ca_fff7_250e_67da_4175_7e92_02ab),
        (69_496, 0x198e_013d_ebc4_ccfe_5b8c_f7a5_16fe_d7c6),
        (71_907, 0xee44_6e55_2bc5_57a7_3a45_0d3a_fb16_4c4b),
        (77_488, 0x04ce_ad56_40bc_476c_85ca_556a_84f7_8d0f),
        (83_626, 0x0186_fb13_43aa_3dd5_a15a_5b53_4542_ffc5),
        (89_207, 0x8763_19bc_dec4_43b6_c49b_f4a4_ab63_e9d9),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (113_764, 0xdc6c_f845_2c21_4bdc_57f1_17d4_ab71_1390),
        (70_890, 0x8ae4_727d_3b59_47a7_5098_4da0_eed1_c7dd),
        (73_301, 0x06f4_42ad_ca88_f36b_8138_8545_2c40_deac),
        (78_882, 0xf465_4901_10b8_3f9b_29da_6e4e_d546_4304),
        (85_020, 0x9156_6ff1_36ec_34f8_0a2d_0ef4_b538_b6e2),
        (90_601, 0x51fa_98c9_02f7_a0d4_d295_969f_f511_453a),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (116_404, 0xd638_58d1_0b52_80aa_fa15_c8c5_7a4d_8cdd),
        (69_859, 0xb8c7_857d_b243_132f_08ba_c573_b651_f794),
        (72_270, 0xf513_cbc1_29dc_89e7_4614_664e_22ac_3aa7),
        (77_851, 0x3b46_5613_c47a_72a2_f45f_7009_9729_6ac7),
        (83_989, 0x3b90_d32d_9dd2_5584_7cbf_8935_fb17_e8b5),
        (89_570, 0xd561_a272_1c7c_8e15_fe35_58a2_5666_b1c5),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (117_798, 0xe4fc_0bc5_769b_6de2_2cd7_9019_af49_7dc6),
        (71_253, 0x151a_7b89_3e2c_79a6_9919_59f5_014d_c5ff),
        (73_664, 0xe592_bb81_4e30_81f0_b5b9_3967_4448_a898),
        (79_245, 0x4a03_73dd_0091_a89b_9a11_c93b_885c_06ec),
        (85_383, 0x36af_6ae1_c9d3_e5a8_5978_6c53_cce9_4ad2),
        (90_964, 0xa635_c831_24bb_85b3_abdc_5840_31ee_2256),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (115_473, 0xd7a3_5255_3b94_fa13_8cbc_0096_1a0e_c0af),
        (71_541, 0xb331_1796_e11b_b2c0_73eb_7d81_b254_db31),
        (73_952, 0x758b_00e3_f010_3163_393f_ca28_99e1_a832),
        (79_533, 0xf4a0_25ad_2119_0f04_cebb_27e8_70dd_d396),
        (85_671, 0xfd36_0bba_17b9_7725_af76_1b49_c9f6_d7a0),
        (91_252, 0x379a_72a5_0488_dae8_9b4f_28f1_dbe8_fedc),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (116_867, 0x360c_e8d3_fe99_45eb_9b3f_91d9_34ea_a0e4),
        (72_935, 0x27ee_86da_aecd_5fd3_20c2_73d0_803d_ebda),
        (75_346, 0x8e74_d4ae_f564_de2f_62e4_59ed_fd8b_b3fd),
        (80_927, 0x42a4_c9cd_e8c3_7e45_5989_91ea_10c0_5285),
        (87_065, 0x8d49_28a5_0013_5803_2fb5_209c_a756_320f),
        (92_646, 0x528d_a3ba_4666_a5a6_7efa_03ca_d4e5_161f),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (119_821, 0x439b_406d_c875_ce53_2c67_8c8e_71ef_1911),
        (71_898, 0x8680_d629_9c55_a78d_56ae_b6b3_1428_25f2),
        (74_309, 0x4ea9_da6f_28f0_0f49_be13_0928_bdaa_be8f),
        (79_890, 0x0a48_ad8f_7ab3_2de8_d323_83f9_18f3_6f93),
        (86_028, 0x016c_c4dd_8d44_91ab_ed7c_a8a0_5307_dc9d),
        (91_609, 0x16e7_9e68_a883_cca9_7028_8451_421e_e321),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (121_215, 0x36b2_4ec9_ea1d_e1b4_55bb_763e_6553_49dc),
        (73_292, 0xeef3_3d9e_39a4_55e8_ed85_124c_27b1_5e1f),
        (75_703, 0xae6e_8671_cf04_abd0_5409_7686_a110_2bf2),
        (81_284, 0x46f8_6569_2dd6_b2c5_ec31_c97d_d414_256e),
        (87_422, 0x53e0_4cf4_4522_6fbf_e377_5f38_200d_5dc4),
        (93_003, 0xfe62_da79_0cfd_c805_41a0_d18e_070d_f2d8),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=Nearest
    [
        (98_960, 0x0121_1258_005d_e4d1_f5e5_85c4_73de_6764),
        (53_485, 0x2560_7c2c_20c0_0715_6819_5a36_71f9_7866),
        (55_896, 0x856a_352b_e4dc_0146_696d_d27c_5187_0cc9),
        (61_477, 0x3350_97c2_765b_04e3_82b1_1c9c_96d1_d9b3),
        (67_615, 0xe398_2338_9db5_d931_9232_03e5_d717_fd11),
        (73_196, 0x19a6_0f03_de8b_27d1_48fe_6bdb_6a3c_7073),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=DepthAwareGather
    [
        (100_354, 0x9dc8_fac8_0d38_1875_8ab0_d277_27fa_7f3b),
        (54_879, 0xd2f9_3844_5aaf_4f31_8db9_d6eb_4b3c_15d5),
        (57_290, 0x3f5a_de72_9353_f094_4b72_6ea4_ee3c_ce2e),
        (62_871, 0xeee7_8095_b2f2_ec17_4433_03df_1ffa_d358),
        (69_009, 0xbdb7_0c15_dd69_2748_714d_3a74_324d_5476),
        (74_590, 0x4d38_8f68_6e51_c3b9_96e0_8813_6675_7650),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=Nearest
    [
        (99_026, 0x3226_9830_aaf7_4779_da7f_e131_8c48_c685),
        (53_551, 0x1bef_0d3e_9ed6_90c3_ecc6_95ce_9afd_b1c7),
        (55_962, 0xf3ff_251d_2d28_b94c_5599_5887_e128_4188),
        (61_543, 0x3fcd_324c_748a_812c_d0b2_f310_fa07_c372),
        (67_681, 0x33f2_32a5_b6ba_34bc_e696_001b_f8bf_7c50),
        (73_262, 0x30c9_254d_4b07_2d48_eb71_0e47_76f2_1f4a),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=DepthAwareGather
    [
        (100_420, 0x55be_fb4a_e021_8062_5489_51b2_bc1f_f65e),
        (54_945, 0x6098_8ee2_f40f_577a_ec7e_dd43_1901_f46c),
        (57_356, 0x5774_2826_0f00_eceb_95fb_90bc_1af8_0e0f),
        (62_937, 0xf950_b356_d04e_9876_2cac_aa69_64ca_46b1),
        (69_075, 0x7d17_e023_7e57_6187_26d2_0231_bbbf_5c4f),
        (74_656, 0x25c0_7d0c_344a_43d8_fa29_957e_651c_2db9),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=Nearest
    [
        (103_308, 0x22f4_d097_1344_44af_7981_9993_3c50_f1e2),
        (53_842, 0xce2e_481f_a2a0_259f_7187_b14d_9e1c_d4a3),
        (56_253, 0x2c8d_c151_016f_6ce1_9422_1f62_2666_c70a),
        (61_834, 0xa1a0_d7d9_9386_7d44_1656_d9ce_ea87_8678),
        (67_972, 0x6f1a_cd0c_df37_aad2_ff9c_c7d8_9658_ad12),
        (73_553, 0xb832_8601_e7b0_a802_9954_272d_ea9d_eca0),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=DepthAwareGather
    [
        (104_702, 0xc43f_6e41_3815_3f9c_80be_2672_e6cc_c1d3),
        (55_236, 0x0474_da9c_d8de_7b76_2d49_06c2_e2e0_f7aa),
        (57_647, 0x9fe5_091b_f42c_57ec_453c_0a52_b436_b483),
        (63_228, 0xb545_ce30_a997_b028_05ba_044e_9d93_61fd),
        (69_366, 0xe4ad_2668_3ba4_1680_c2c2_73df_cc2b_a37b),
        (74_947, 0xa38e_7832_3791_545f_6d0f_83aa_f3b9_1efd),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=Nearest
    [
        (103_374, 0xec45_517a_58fd_cff0_332f_8250_2324_2971),
        (53_908, 0x1e7b_a9c9_1e3b_eb55_6c77_35da_b66d_e8de),
        (56_319, 0x6ec8_66cc_0ab1_05dc_e2ed_68c5_0aef_93cf),
        (61_900, 0xf0b9_8212_0f2b_fe15_988d_08e6_14cd_0a05),
        (68_038, 0x2da5_d317_d83a_dffe_3800_2041_8c4e_da5f),
        (73_619, 0x335d_aecd_b87d_e8e9_744b_82c4_f1ec_373d),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=DepthAwareGather
    [
        (104_768, 0x52e9_395e_89b3_c769_6ea7_891d_356b_bca4),
        (55_302, 0x13e1_ba4c_0761_1a31_4767_2d83_8089_56e7),
        (57_713, 0xfe1d_3739_d666_fde3_0d87_a166_32b0_d4d6),
        (63_294, 0x6821_9ca1_2e3e_99a9_c7bd_3a96_993e_5a80),
        (69_432, 0xf37a_8682_ccbb_84f5_7933_5e28_a498_0976),
        (75_013, 0xc1bf_b72a_fc4c_704e_dd0f_6520_da67_f430),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=Nearest
    [
        (115_473, 0xd7a3_5255_3b94_fa13_8cbc_0096_1a0e_c0af),
        (71_541, 0xb331_1796_e11b_b2c0_73eb_7d81_b254_db31),
        (73_952, 0x758b_00e3_f010_3163_393f_ca28_99e1_a832),
        (79_533, 0xf4a0_25ad_2119_0f04_cebb_27e8_70dd_d396),
        (85_671, 0xfd36_0bba_17b9_7725_af76_1b49_c9f6_d7a0),
        (91_252, 0x379a_72a5_0488_dae8_9b4f_28f1_dbe8_fedc),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (116_867, 0x360c_e8d3_fe99_45eb_9b3f_91d9_34ea_a0e4),
        (72_935, 0x27ee_86da_aecd_5fd3_20c2_73d0_803d_ebda),
        (75_346, 0x8e74_d4ae_f564_de2f_62e4_59ed_fd8b_b3fd),
        (80_927, 0x42a4_c9cd_e8c3_7e45_5989_91ea_10c0_5285),
        (87_065, 0x8d49_28a5_0013_5803_2fb5_209c_a756_320f),
        (92_646, 0x528d_a3ba_4666_a5a6_7efa_03ca_d4e5_161f),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (115_539, 0x15b6_4f73_8ef8_9e3e_0341_3af4_f0e9_2b1a),
        (71_607, 0x1fec_4a44_6d0b_c2e0_8aab_74f5_c4d9_5520),
        (74_018, 0x9323_3513_480a_306b_62ba_125c_4d61_506b),
        (79_599, 0x1b9e_6b46_27c0_be1b_3c71_9c66_a239_0727),
        (85_737, 0x1cf4_7d1f_df7d_0564_ad72_dc16_f324_2e99),
        (91_318, 0x90e2_3ae2_6a9a_6aa1_0a4f_0eaa_d2dd_4f8d),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (116_933, 0x1fdd_9ee6_0d83_5edf_a275_e885_46cc_c1bd),
        (73_001, 0x6c8d_ea85_0c74_20c6_7522_608a_225c_11f3),
        (75_412, 0xbf94_eda1_f714_f3bd_a85b_504e_05a6_1a04),
        (80_993, 0x6126_9283_d40c_6ad3_1f6e_bfa4_9d51_921c),
        (87_131, 0x9f85_72a1_69b2_7ae2_0041_e149_033e_444e),
        (92_712, 0xccf5_6ea0_48b1_716b_cf75_1b27_479b_370e),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (119_821, 0x439b_406d_c875_ce53_2c67_8c8e_71ef_1911),
        (71_898, 0x8680_d629_9c55_a78d_56ae_b6b3_1428_25f2),
        (74_309, 0x4ea9_da6f_28f0_0f49_be13_0928_bdaa_be8f),
        (79_890, 0x0a48_ad8f_7ab3_2de8_d323_83f9_18f3_6f93),
        (86_028, 0x016c_c4dd_8d44_91ab_ed7c_a8a0_5307_dc9d),
        (91_609, 0x16e7_9e68_a883_cca9_7028_8451_421e_e321),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (121_215, 0x36b2_4ec9_ea1d_e1b4_55bb_763e_6553_49dc),
        (73_292, 0xeef3_3d9e_39a4_55e8_ed85_124c_27b1_5e1f),
        (75_703, 0xae6e_8671_cf04_abd0_5409_7686_a110_2bf2),
        (81_284, 0x46f8_6569_2dd6_b2c5_ec31_c97d_d414_256e),
        (87_422, 0x53e0_4cf4_4522_6fbf_e377_5f38_200d_5dc4),
        (93_003, 0xfe62_da79_0cfd_c805_41a0_d18e_070d_f2d8),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (119_887, 0xe31d_833f_1065_ee03_6b95_2b00_d0d5_4136),
        (71_964, 0x584a_7221_1a1d_d32e_9028_78a4_ff35_1527),
        (74_375, 0xe68e_93d4_4b6e_4bb9_9848_3c19_e911_c442),
        (79_956, 0x81d9_080e_7435_0cfc_211d_64f0_a39d_4d0e),
        (86_094, 0xb6eb_e819_3d9a_eb96_800e_5c51_42bd_c248),
        (91_675, 0x5d4e_c7c7_8406_97dc_abd7_bfe8_d2f3_b9b4),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (121_281, 0x9667_825d_2fb6_e350_fae1_f55f_5e7c_dea7),
        (73_358, 0xb14d_9d97_7a89_0aea_4ad6_c2ff_478d_564a),
        (75_769, 0x38d9_792e_aa8f_3fff_1a0d_ea62_de88_871f),
        (81_350, 0x6bbe_bc17_38e0_9b78_c7df_a95b_0c6f_2673),
        (87_488, 0x8f1f_0d4c_c521_7c74_d447_1cdf_c0dc_40f1),
        (93_069, 0x83a4_b8ad_9215_174b_6193_937a_ac3b_35fd),
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
    (95_427, 0x1c12_10e2_53d2_2181_6404_273f_7709_b1c2),
    (95_846, 0x5cdc_9460_6568_5bbd_3ac7_6251_3b73_823e),
    (97_240, 0xace8_a419_501d_e6b9_f0ba_da61_8f7f_e96d),
    (99_461, 0x0815_8c81_953b_f13a_b720_e988_8312_6838),
    (99_880, 0x8d89_67b4_00b7_2b8d_a57d_000c_4c24_72d8),
    (101_274, 0x9a72_266b_f334_707f_a91e_cace_46a1_892b),
    (111_940, 0xc60e_4e8e_55d3_db53_9753_1c9b_8be8_6d9f),
    (115_974, 0xdb67_e6f2_968d_26c4_9687_c1bd_d18d_34fd),
    (115_462, 0x76a3_1ee1_a71c_80e0_f96e_65d4_66e9_5cc9),
    (116_856, 0x4059_c488_7d1b_441a_780f_b0ec_1b8a_cd86),
    (119_810, 0x5b04_dcbb_1a95_2dc7_875e_544a_96d3_eedb),
    (121_204, 0x6d68_91b7_af70_3e98_a725_af08_8452_027e),
    (112_359, 0x74c4_6807_7654_8360_d56c_58c6_97b3_0d13),
    (113_753, 0xb6f7_8fd9_e11f_6e6a_1f96_8be4_e302_bc00),
    (116_393, 0x4d98_6048_b660_1cd2_6b43_666b_73d1_cbc5),
    (117_787, 0x7167_f6af_4cff_23c9_9453_1bd5_0820_5146),
    (115_462, 0x76a3_1ee1_a71c_80e0_f96e_65d4_66e9_5cc9),
    (116_856, 0x4059_c488_7d1b_441a_780f_b0ec_1b8a_cd86),
    (119_810, 0x5b04_dcbb_1a95_2dc7_875e_544a_96d3_eedb),
    (121_204, 0x6d68_91b7_af70_3e98_a725_af08_8452_027e),
    (98_949, 0x9e39_a781_20c8_89bc_d964_997c_85d6_f91c),
    (100_343, 0xc502_71f3_5641_647f_72a7_7e8a_ef34_563f),
    (99_015, 0xccab_bf23_476e_51b8_0cd5_0476_3932_5a8d),
    (100_409, 0xa609_d9df_c019_5d80_b4e3_824d_48a8_340a),
    (103_297, 0x1627_eaea_f07d_9490_7eb0_b841_02c6_3fba),
    (104_691, 0x3c06_d115_1188_a14c_e5e1_3067_12d0_9ddf),
    (103_363, 0xc149_3efc_f439_9338_d3e9_9b98_f800_49d9),
    (104_757, 0x551d_72ad_d1c2_290b_9d07_eb84_a270_b418),
    (115_462, 0x76a3_1ee1_a71c_80e0_f96e_65d4_66e9_5cc9),
    (116_856, 0x4059_c488_7d1b_441a_780f_b0ec_1b8a_cd86),
    (115_528, 0x5883_955b_ce41_5621_a636_e8de_95a8_326c),
    (116_922, 0x5a7a_a406_1731_487f_5476_3d71_cd09_add7),
    (119_810, 0x5b04_dcbb_1a95_2dc7_875e_544a_96d3_eedb),
    (121_204, 0x6d68_91b7_af70_3e98_a725_af08_8452_027e),
    (119_876, 0xa886_21fb_8574_cd4b_a31b_fdfe_33d8_2f80),
    (121_270, 0x35e9_3bf4_3eb3_24a7_48bb_4a46_b9dd_7501),
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
