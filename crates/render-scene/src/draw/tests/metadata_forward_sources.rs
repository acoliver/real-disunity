//! Complete forward source assemblies, including their final screen-space and terrain rewrites.

// Expected byte lengths and FNV-1a/128 from production assembly with invariant positions.
// Each row is object, terrain, then conventional/cached/layered/cached-layered terrain.
const FINAL_FORWARD_SOURCES: &[[(usize, u128); 6]] = &[
    // ordinary velocity=false ssao=false receiver=Nearest
    [
        (93_482, 0x22e8_31dc_934f_cb24_2e4f_660c_23cb_6188),
        (48_980, 0xa055_4b91_69fa_ff16_c593_ee7c_7301_a789),
        (51_391, 0xa5dd_7650_1bc0_1ade_6574_89a1_22b2_48e0),
        (56_787, 0x6754_4b60_9a9b_a305_f65e_dfe0_d0aa_4c7d),
        (63_110, 0xb38d_f7d5_73fa_8bef_fe22_fc29_206a_6686),
        (68_506, 0xfea0_15fa_3163_af79_6e07_3fbb_da10_5c57),
    ],
    // ordinary velocity=false ssao=true receiver=Nearest
    [
        (93_901, 0xa129_c0de_4c4b_74bd_fd3f_4320_98ae_81aa),
        (49_399, 0x7134_c45b_920e_b087_63c2_b2f6_3814_dac5),
        (51_810, 0xe25f_ceaa_7706_ce70_dffb_a9e5_9dc2_1a56),
        (57_206, 0x6369_57e1_e4fb_50e9_3859_7cd5_cfdc_98fd),
        (63_529, 0x29d6_3068_8200_ffb9_74d3_39e3_7b01_ace6),
        (68_925, 0x8885_1fd2_6bac_4901_be6a_5441_3a15_acfd),
    ],
    // ordinary velocity=false ssao=true receiver=DepthAwareGather
    [
        (95_295, 0xc6b6_7746_5a6b_38d0_0960_4c28_6184_1701),
        (50_793, 0xc083_7cf2_c1de_4e49_67dd_346e_4d53_9136),
        (53_204, 0x51e9_bd76_e7e0_cfad_2ae2_c03e_836e_e441),
        (58_600, 0x4a84_b963_fb12_449b_11ae_9164_e03e_ab80),
        (64_923, 0xaaae_3f9f_d81f_b098_af56_6e5f_ec4f_8c81),
        (70_319, 0xe31e_5db0_b21d_e8aa_3c35_f267_ba25_22d8),
    ],
    // ordinary velocity=true ssao=false receiver=Nearest
    [
        (97_516, 0x9d48_4777_7456_6105_da55_1b14_468e_f896),
        (49_343, 0x82c0_621d_51b9_75d9_54da_286a_e58e_6a5b),
        (51_754, 0x88d3_051d_2928_78eb_5580_f73d_55c7_fb18),
        (57_150, 0x061e_7106_70a6_c07b_c0f5_59e2_231c_abf1),
        (63_473, 0xf60f_4c98_f9b4_ba11_e67f_f772_d695_2a02),
        (68_869, 0x9eb4_9927_8ae0_1ab6_d6c4_a08e_d4c0_faa7),
    ],
    // ordinary velocity=true ssao=true receiver=Nearest
    [
        (97_935, 0x4ec9_638b_7def_5d2e_5ae9_5295_2cc4_8504),
        (49_762, 0x8084_44f2_efd7_2f73_506e_fc19_4fbb_cabd),
        (52_173, 0xfb78_f5f4_163d_d203_1d68_9907_2e6b_a584),
        (57_569, 0xe1e0_876a_5cdf_dee4_e209_ed8b_0a66_d933),
        (63_892, 0x25fd_576e_7d6d_ccd8_cc2e_7290_4224_2f94),
        (69_288, 0xa48f_c369_de9f_e641_6ff8_18ed_c63b_ec23),
    ],
    // ordinary velocity=true ssao=true receiver=DepthAwareGather
    [
        (99_329, 0xcc1b_19f0_6626_aa1f_3046_95ce_99a1_96bf),
        (51_156, 0xea7e_f475_aa41_07c6_9550_dede_c68d_5b86),
        (53_567, 0xb4be_e6c9_a254_15f9_90d1_f368_0252_dfbb),
        (58_963, 0xf970_f047_17db_c6bf_b7f0_0519_92e4_9672),
        (65_286, 0x4f13_e372_e51a_0bc6_ca1c_e40b_2992_1cf3),
        (70_682, 0x5868_3b4c_9dcd_6506_fffb_4ac9_9b4b_71a2),
    ],
    // receiver ssao=false reflection=false velocity=false receiver=Nearest
    [
        (106_683, 0x77cf_17be_2120_bab9_7b66_5361_2ff7_fcb2),
        (62_181, 0x38c8_3d41_12bf_5c57_5db5_8d12_5e22_103b),
        (64_592, 0x6920_2ec0_59ab_2634_442f_1ef0_efff_7c5c),
        (69_988, 0x3d51_086e_48dd_9627_51de_4289_42e9_2449),
        (76_311, 0x07da_3fb8_383c_07d8_7d22_d080_fffe_3dd2),
        (81_707, 0xff9d_228b_113e_1c80_289e_9a47_d385_a4e3),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (110_717, 0xf81f_d0e7_8924_d55f_39b2_8297_6af9_c890),
        (62_544, 0x40f9_a612_a765_4297_574f_0fd5_1f46_a879),
        (64_955, 0x27d6_6283_2d93_f3bd_c7c5_2dd8_79d6_c2ac),
        (70_351, 0xbc97_804d_d860_f2f7_69a6_75e7_d49c_36f5),
        (76_674, 0xe181_06b0_e565_4f8f_7bab_fc3c_c80e_ca36),
        (82_070, 0xfffa_dd6b_ea38_835a_389c_bb31_d34d_27ab),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (110_205, 0xc0a4_90af_0a6d_f4fa_0c02_b3bf_a2b4_c380),
        (64_645, 0x9cc4_5d1f_e297_4386_7c04_913a_428b_3f94),
        (67_056, 0xdcae_b453_d288_38aa_fc6c_9312_8b31_5ebb),
        (72_452, 0xdd81_dfdf_f9ad_3859_3af7_a833_0c7b_071e),
        (78_775, 0x0d7a_f063_1ff9_fa90_ba1b_a499_3c80_08c3),
        (84_171, 0x4483_1bef_344f_a8c4_d229_ed47_672c_dfee),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (111_599, 0x5a55_9e83_3e20_8728_48ba_f8ba_bfe3_5dd3),
        (66_039, 0x3183_f23c_afb8_7d93_3449_2ceb_5740_319b),
        (68_450, 0xd283_723c_7188_7fe6_cb87_b6bc_3798_b0c8),
        (73_846, 0x761e_764c_c091_63d4_c9c9_e739_b01e_12eb),
        (80_169, 0x55bc_0ab9_2837_da46_a14e_0049_fcc3_2d78),
        (85_565, 0xca7c_5026_b2ae_21d1_04e5_b64d_812b_4f4b),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (114_553, 0x2c5d_1b8a_4b66_57d7_a3b0_397f_5866_2d6e),
        (65_002, 0xf5c8_8fc5_7d8b_9af4_a423_cf70_bdba_f679),
        (67_413, 0x313b_9e0c_9834_faa4_da3b_9ca9_1db9_032c),
        (72_809, 0x8f62_c35e_63c9_7c8b_7a32_5465_2af6_6043),
        (79_132, 0x5871_9f46_6228_b22d_d375_0fa7_9e6a_eb4c),
        (84_528, 0x86cd_350b_c179_7038_dc7d_0011_a291_7a13),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (115_947, 0xa225_04af_ae93_b435_5a90_b018_dc42_c7bb),
        (66_396, 0xcf9e_0807_2775_f3ac_31ce_a972_8600_9168),
        (68_807, 0x32e5_0a38_839f_fc88_9684_8b80_476d_76a5),
        (74_203, 0x4468_9987_3cb7_9409_8811_7d35_5a50_d90c),
        (80_526, 0x4f9d_efed_3936_bf10_1ddd_01bc_b7cc_d9ad),
        (85_922, 0x7e54_0a1f_89c7_19d2_8a7f_fb30_ea74_cd6c),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (107_102, 0x3847_6779_d02b_badc_7e67_eaeb_7e78_5ec2),
        (62_600, 0x0d9b_fd0d_328c_a0f2_9e88_5fbe_eead_6a9d),
        (65_011, 0x68b5_6d6b_a575_3026_6228_dedb_4771_aea0),
        (70_407, 0xcc0e_3059_ff7e_7ee4_2676_d3f3_8cbb_2a33),
        (76_730, 0xf556_8073_fb13_be7e_d73e_3f09_fe4c_2108),
        (82_126, 0x0127_949a_b4b9_989e_2029_d97e_7382_3a5b),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (108_496, 0xbe9a_c007_fc0d_2141_e0c5_1ceb_04c5_200d),
        (63_994, 0x5bfb_e727_386b_b236_814e_f0a7_f0dd_d9da),
        (66_405, 0x61b5_b933_5d2e_7264_53a8_54b6_766d_affb),
        (71_801, 0x0f9a_1742_b91c_a5f1_f69f_6623_bc78_2c96),
        (78_124, 0x11b3_878f_32aa_fbdf_72cd_4d5d_de03_110b),
        (83_520, 0x0985_4544_9cc2_daaf_c52e_368c_744d_e03e),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (111_136, 0xa1c1_ac4c_ff07_f23f_27ec_4138_29fa_3eec),
        (62_963, 0xd2bb_26dd_c597_7026_0eb5_0834_8be9_5755),
        (65_374, 0xc244_a34b_5872_d9fa_2cbd_9e41_9230_6fba),
        (70_770, 0x282f_a303_61e8_8df2_855f_51a9_0db6_a68d),
        (77_093, 0xb906_6c0e_eca4_9f28_5a36_f78f_3234_3e12),
        (82_489, 0x66a9_dc76_6492_65ee_25c9_ea51_2508_d965),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (112_530, 0xd480_7100_f0fe_e474_e13b_f2df_7a6a_2853),
        (64_357, 0x602f_2067_e7f6_bb99_4432_0c19_61f4_e3c2),
        (66_768, 0x2ca1_4102_bac5_696b_1822_ab1a_774a_7b31),
        (72_164, 0x9917_dc64_d687_602c_209d_b247_2159_461c),
        (78_487, 0xcd64_caff_47b8_964a_c7b3_e842_3677_5329),
        (83_883, 0xfd85_4080_2d72_471a_6b56_cf76_81e9_ef0c),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (110_205, 0xc0a4_90af_0a6d_f4fa_0c02_b3bf_a2b4_c380),
        (64_645, 0x9cc4_5d1f_e297_4386_7c04_913a_428b_3f94),
        (67_056, 0xdcae_b453_d288_38aa_fc6c_9312_8b31_5ebb),
        (72_452, 0xdd81_dfdf_f9ad_3859_3af7_a833_0c7b_071e),
        (78_775, 0x0d7a_f063_1ff9_fa90_ba1b_a499_3c80_08c3),
        (84_171, 0x4483_1bef_344f_a8c4_d229_ed47_672c_dfee),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (111_599, 0x5a55_9e83_3e20_8728_48ba_f8ba_bfe3_5dd3),
        (66_039, 0x3183_f23c_afb8_7d93_3449_2ceb_5740_319b),
        (68_450, 0xd283_723c_7188_7fe6_cb87_b6bc_3798_b0c8),
        (73_846, 0x761e_764c_c091_63d4_c9c9_e739_b01e_12eb),
        (80_169, 0x55bc_0ab9_2837_da46_a14e_0049_fcc3_2d78),
        (85_565, 0xca7c_5026_b2ae_21d1_04e5_b64d_812b_4f4b),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (114_553, 0x2c5d_1b8a_4b66_57d7_a3b0_397f_5866_2d6e),
        (65_002, 0xf5c8_8fc5_7d8b_9af4_a423_cf70_bdba_f679),
        (67_413, 0x313b_9e0c_9834_faa4_da3b_9ca9_1db9_032c),
        (72_809, 0x8f62_c35e_63c9_7c8b_7a32_5465_2af6_6043),
        (79_132, 0x5871_9f46_6228_b22d_d375_0fa7_9e6a_eb4c),
        (84_528, 0x86cd_350b_c179_7038_dc7d_0011_a291_7a13),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (115_947, 0xa225_04af_ae93_b435_5a90_b018_dc42_c7bb),
        (66_396, 0xcf9e_0807_2775_f3ac_31ce_a972_8600_9168),
        (68_807, 0x32e5_0a38_839f_fc88_9684_8b80_476d_76a5),
        (74_203, 0x4468_9987_3cb7_9409_8811_7d35_5a50_d90c),
        (80_526, 0x4f9d_efed_3936_bf10_1ddd_01bc_b7cc_d9ad),
        (85_922, 0x7e54_0a1f_89c7_19d2_8a7f_fb30_ea74_cd6c),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=Nearest
    [
        (97_004, 0x3671_0718_ad4d_29a2_d276_4bf9_3b21_4892),
        (51_444, 0x02c2_ae4d_4e3e_b225_cebd_c644_8056_22f0),
        (53_855, 0x1994_f033_7173_9f08_5038_830d_d984_6951),
        (59_251, 0xa1b9_c134_91eb_4f5f_2a4d_c0e9_7194_14d8),
        (65_574, 0xdcbc_0bbf_799d_4ee9_7671_3f83_2776_80f9),
        (70_970, 0x2ad2_0fd8_cbbc_4732_2231_8ccc_0c06_d648),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=DepthAwareGather
    [
        (98_398, 0xac9d_7f89_f8e9_96f6_ba25_9258_1f99_34bd),
        (52_838, 0x2bf2_c3e6_1292_fabf_e766_2efd_dc43_d46b),
        (55_249, 0xffcb_6860_9133_1d0c_2d50_a2fa_b853_8486),
        (60_645, 0x2171_5e2c_0d19_ee77_9d3d_049c_1456_d38d),
        (66_968, 0x42b4_2eba_7e83_29be_70b2_4c4b_5e0b_a53e),
        (72_364, 0x2d8c_e5d3_0133_80b4_df2a_d48e_6e49_cae5),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=Nearest
    [
        (97_070, 0xca78_e2a8_df73_c438_8480_7ec0_2496_7bb3),
        (51_510, 0x8dfb_b306_dc40_952e_aed2_fc75_6a39_d739),
        (53_921, 0x1983_0d7e_b0ae_fa78_9c8c_278c_b622_2040),
        (59_317, 0x6807_fa43_00cd_7e50_32f1_023c_9e4b_3993),
        (65_640, 0x60a3_d37f_9e26_b48c_ebfd_8850_ffe9_17e8),
        (71_036, 0xb1d1_c1d4_021a_fbe3_8d04_08e0_e7cb_ff1b),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=DepthAwareGather
    [
        (98_464, 0x000a_3df6_a182_a474_92d1_2c99_2c51_6e38),
        (52_904, 0x3040_c415_029f_ba3b_520d_121c_7ccc_e03a),
        (55_315, 0x83ee_1825_4014_dccc_1280_bc43_5843_96d7),
        (60_711, 0x5ead_b612_f6ad_5011_b667_cd8e_9125_8c9a),
        (67_034, 0x07be_8e6d_f453_2b72_7c1b_ed56_e4c8_2507),
        (72_430, 0x27a0_1e31_e523_70ee_0df9_a111_c4b9_7662),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=Nearest
    [
        (101_352, 0x1152_d7d5_378c_be46_e0f8_5f81_b8a5_1560),
        (51_801, 0xf98f_d39e_2d3b_4d55_9633_69fa_4124_7035),
        (54_212, 0x9298_0144_2bc2_7aac_465d_73c2_6c33_bbd2),
        (59_608, 0x1760_2fd2_b29d_70e0_3d52_6f61_f739_fb5d),
        (65_931, 0xb060_0ed2_9782_de00_07c6_c3e1_8959_3f7a),
        (71_327, 0x4615_eb75_1118_0404_7b6c_ef7e_8256_6515),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=DepthAwareGather
    [
        (102_746, 0x80bd_7abc_fdfc_1da7_18d1_78c5_6910_8715),
        (53_195, 0x3aab_a5cb_5485_7e31_15c6_6296_26d5_a698),
        (55_606, 0x95c0_7e88_1578_0a8c_8a6c_d6a1_1593_639b),
        (61_002, 0xddf8_6c2b_386d_6589_0701_4b03_afa7_603e),
        (67_325, 0xa04b_593c_98ac_be69_4676_d988_f7dd_ce43),
        (72_721, 0xce0d_7244_4a8f_868e_c10f_7d16_90f8_19fe),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=Nearest
    [
        (101_418, 0x241a_48d2_1a34_1ad4_e28c_b428_00fb_e4bf),
        (51_867, 0x0c16_8bb3_422f_cc27_2c74_1220_56af_c2d0),
        (54_278, 0xfb9a_5797_a661_a507_4e83_7fbc_12f3_0287),
        (59_674, 0x58f4_01b9_4fed_d277_15ec_94ad_24d7_fa76),
        (65_997, 0x930a_3335_a990_63b7_08ba_b33c_a95f_0617),
        (71_393, 0x12c5_2dc1_b514_00bf_21a4_6784_6dfa_227e),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=DepthAwareGather
    [
        (102_812, 0x0cea_ed31_b205_5d15_7730_a02f_9918_8166),
        (53_261, 0x79f2_8487_a9a7_105b_ace5_ba19_5e5d_106d),
        (55_672, 0x7654_b93c_5190_78e5_38ad_2dd5_7562_a6be),
        (61_068, 0x6912_c1dd_6e7f_f5f1_9dc0_d1e1_59ce_5f75),
        (67_391, 0x28f8_65a2_8db9_2dac_0a8b_6259_8fff_8e2e),
        (72_787, 0x4534_8d96_228e_87cd_8423_160a_f299_1ac5),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=Nearest
    [
        (110_205, 0xc0a4_90af_0a6d_f4fa_0c02_b3bf_a2b4_c380),
        (64_645, 0x9cc4_5d1f_e297_4386_7c04_913a_428b_3f94),
        (67_056, 0xdcae_b453_d288_38aa_fc6c_9312_8b31_5ebb),
        (72_452, 0xdd81_dfdf_f9ad_3859_3af7_a833_0c7b_071e),
        (78_775, 0x0d7a_f063_1ff9_fa90_ba1b_a499_3c80_08c3),
        (84_171, 0x4483_1bef_344f_a8c4_d229_ed47_672c_dfee),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (111_599, 0x5a55_9e83_3e20_8728_48ba_f8ba_bfe3_5dd3),
        (66_039, 0x3183_f23c_afb8_7d93_3449_2ceb_5740_319b),
        (68_450, 0xd283_723c_7188_7fe6_cb87_b6bc_3798_b0c8),
        (73_846, 0x761e_764c_c091_63d4_c9c9_e739_b01e_12eb),
        (80_169, 0x55bc_0ab9_2837_da46_a14e_0049_fcc3_2d78),
        (85_565, 0xca7c_5026_b2ae_21d1_04e5_b64d_812b_4f4b),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (110_271, 0x5945_21a1_6848_bb59_d091_c3e4_fc42_bc09),
        (64_711, 0x325f_71ae_3f84_630b_ead8_e1e7_f237_fbf5),
        (67_122, 0xabb0_28de_971c_693d_b449_0dc6_1bf4_e80a),
        (72_518, 0x6d12_1d5e_a6ce_6af0_faad_598c_3b89_8c01),
        (78_841, 0x9f66_c7a9_9d12_e665_733f_dbd6_6532_0aca),
        (84_237, 0x4e22_8e8e_53b6_3027_5bca_8bac_3546_fd01),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (111_665, 0x7a31_c216_1480_fd7b_d5b6_cd8a_b3b6_60c6),
        (66_105, 0x473f_07ec_d6a8_2362_df31_842d_c069_4cea),
        (68_516, 0x6acb_0613_02db_0c45_d742_af8e_6e48_8021),
        (73_912, 0xa6c4_b965_a771_2543_9908_cc77_0000_5498),
        (80_235, 0x428a_ab17_076b_a226_92a1_dfbc_6dc6_7311),
        (85_631, 0x3419_b625_2967_baae_08e9_8a49_fc72_d1a0),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (114_553, 0x2c5d_1b8a_4b66_57d7_a3b0_397f_5866_2d6e),
        (65_002, 0xf5c8_8fc5_7d8b_9af4_a423_cf70_bdba_f679),
        (67_413, 0x313b_9e0c_9834_faa4_da3b_9ca9_1db9_032c),
        (72_809, 0x8f62_c35e_63c9_7c8b_7a32_5465_2af6_6043),
        (79_132, 0x5871_9f46_6228_b22d_d375_0fa7_9e6a_eb4c),
        (84_528, 0x86cd_350b_c179_7038_dc7d_0011_a291_7a13),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (115_947, 0xa225_04af_ae93_b435_5a90_b018_dc42_c7bb),
        (66_396, 0xcf9e_0807_2775_f3ac_31ce_a972_8600_9168),
        (68_807, 0x32e5_0a38_839f_fc88_9684_8b80_476d_76a5),
        (74_203, 0x4468_9987_3cb7_9409_8811_7d35_5a50_d90c),
        (80_526, 0x4f9d_efed_3936_bf10_1ddd_01bc_b7cc_d9ad),
        (85_922, 0x7e54_0a1f_89c7_19d2_8a7f_fb30_ea74_cd6c),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (114_619, 0xd4ce_1c8a_ccd5_98ae_fdbb_e616_2e07_acc5),
        (65_068, 0xe5c0_656b_1581_7bd5_3de2_55f3_7619_2f1c),
        (67_479, 0xeb40_df7a_4b75_cadf_5fa5_03d4_02e9_55c1),
        (72_875, 0x5dee_d4f5_6b75_0a8c_b858_e487_9088_e364),
        (79_198, 0x176e_c6e3_6cbe_18c1_e643_0f7b_fb68_f541),
        (84_594, 0x2978_6b05_371b_9096_ed6e_df70_5579_0dfc),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (116_013, 0x27dd_503b_985c_1f7d_6c0b_d305_99e2_e97c),
        (66_462, 0x6396_7f34_b93e_353c_964c_c5da_53ac_acbd),
        (68_873, 0xfaf4_e9ee_8fea_a0bd_c6d7_874e_3a63_4208),
        (74_269, 0x418d_cfec_2ad8_4976_baa4_cefa_076c_ed0b),
        (80_592, 0xfb25_1365_710e_b884_8a83_963a_0963_f3b0),
        (85_988, 0xfa0a_c73c_b2fd_1ebb_486e_8f3b_99d1_1ac3),
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

// Unpaired quad assemblies keep main's non-invariant position compilation.
const UNLIT_QUAD_SOURCES: &[(usize, u128)] = &[
    (93_471, 0x5509_e28c_a0cc_bfd1_606a_bd83_5ba0_9dd0),
    (93_890, 0x4743_4933_7d2b_c547_5ee9_495f_b153_47b4),
    (95_284, 0x7e21_a164_40f9_ca84_72f2_91e2_e919_942f),
    (97_505, 0xde2f_7433_7bf2_f29a_dab5_0e18_3c04_d27e),
    (97_924, 0xd2f3_4467_a6ba_b35e_c98c_588e_1ff2_3d2e),
    (99_318, 0x7064_ac3b_354e_6abb_22df_d65e_1b71_af25),
    (106_672, 0x574a_0fce_f429_89b1_5e5d_b2b5_1952_265e),
    (110_706, 0x607c_617c_9577_059e_1b48_069e_5e86_4ea4),
    (110_194, 0x63cc_1147_4b41_d381_854e_eebb_16b9_d40c),
    (111_588, 0xab3c_c195_fc3d_5363_70d9_ed79_2b5d_9453),
    (114_542, 0xa7f8_336a_e65f_f5c1_f6ed_5d27_1596_6f02),
    (115_936, 0xd31d_9d72_014f_0aa7_8e41_e367_89ec_0dab),
    (107_091, 0x3486_5c30_5407_b33a_bc42_cd81_656f_423c),
    (108_485, 0xb756_93bc_4732_7780_ea3c_b53d_fa68_3623),
    (111_125, 0x05cf_1a36_9af7_3aa1_2f28_654e_b202_8bc6),
    (112_519, 0x5500_b6aa_f103_589e_d9b9_ddaf_4272_e051),
    (110_194, 0x63cc_1147_4b41_d381_854e_eebb_16b9_d40c),
    (111_588, 0xab3c_c195_fc3d_5363_70d9_ed79_2b5d_9453),
    (114_542, 0xa7f8_336a_e65f_f5c1_f6ed_5d27_1596_6f02),
    (115_936, 0xd31d_9d72_014f_0aa7_8e41_e367_89ec_0dab),
    (96_993, 0x00a5_ee5b_9db2_a532_29c8_49b1_e3d3_ad7a),
    (98_387, 0x96c4_da16_ac82_20e4_528e_8688_8e92_4451),
    (97_059, 0xd587_eedf_51b3_d942_8eed_45f1_6289_406b),
    (98_453, 0x21c9_a714_8c85_e495_3807_8090_4834_9154),
    (101_341, 0x3431_f75c_619f_ada3_23b4_b908_840e_f5c8),
    (102_735, 0xf231_b2a0_6253_2ee4_9644_4489_49b8_0651),
    (101_407, 0x10cb_3513_a9cb_a365_45c3_c0d1_9b49_e6b7),
    (102_801, 0x17e2_a4b9_ddbc_4345_552d_293c_c3c7_7f3a),
    (110_194, 0x63cc_1147_4b41_d381_854e_eebb_16b9_d40c),
    (111_588, 0xab3c_c195_fc3d_5363_70d9_ed79_2b5d_9453),
    (110_260, 0x9d9b_9ad6_37b4_23c9_e1c2_2d06_d979_32bd),
    (111_654, 0xd1c1_5c29_b6c6_a000_9330_0fe9_0f39_f346),
    (114_542, 0xa7f8_336a_e65f_f5c1_f6ed_5d27_1596_6f02),
    (115_936, 0xd31d_9d72_014f_0aa7_8e41_e367_89ec_0dab),
    (114_608, 0x9002_1c4c_9138_5732_1e47_e85f_4684_3a91),
    (116_002, 0x1599_b1a2_a1a7_0c3e_cb7a_7f5f_8cc1_372c),
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
