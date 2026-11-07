//! Complete forward source assemblies, including their final screen-space and terrain rewrites.

// Expected byte lengths and FNV-1a/128 from production assembly with invariant positions.
// Each row is object, terrain, then conventional/cached/layered/cached-layered terrain.
const FINAL_FORWARD_SOURCES: &[[(usize, u128); 6]] = &[
    // ordinary velocity=false ssao=false receiver=Nearest
    [
        (92_624, 0x13f7_772b_8a0c_c723_48a1_1235_fc7c_e642),
        (48_122, 0xa5b9_71f2_f89d_69d5_2594_003f_fb58_598b),
        (50_533, 0xc5c3_fb51_b7e6_6138_7230_bd1a_0329_ff96),
        (55_929, 0x8a4b_1673_1764_2a34_7539_f294_640a_03af),
        (62_252, 0x8f6f_fcf8_535d_f542_fec5_25c1_0f55_62d4),
        (67_648, 0xe61f_6ba1_6151_3105_72fe_4263_1da2_1099),
    ],
    // ordinary velocity=false ssao=true receiver=Nearest
    [
        (93_043, 0x8782_4af9_f739_9c3f_ad18_39d3_d475_bd84),
        (48_541, 0xd60b_fcdf_072e_9a3a_10bb_787d_d19c_e88f),
        (50_952, 0x7bc9_e1b7_569d_7bfe_a560_80b3_3177_2e6c),
        (56_348, 0xa1cd_cc96_c623_f8d4_dd78_aca9_9d65_e56f),
        (62_671, 0xa275_0f56_6274_e00d_96e4_7586_18c1_c334),
        (68_067, 0xb581_54c4_985c_20b4_8fb7_6e57_dac2_3d67),
    ],
    // ordinary velocity=false ssao=true receiver=DepthAwareGather
    [
        (94_437, 0xf89c_9d4c_cfed_7e59_e3a0_57c2_6c1e_e177),
        (49_935, 0xb3da_d079_f087_9313_e3dc_1753_7fe1_2d64),
        (52_346, 0x4a57_e98f_7787_3ca6_c894_91a8_d45a_8f33),
        (57_742, 0xd7f1_378d_b9a9_2de4_5d51_07b1_dbbb_4ade),
        (64_065, 0xc481_e262_11ee_f029_0df3_ba2e_4e70_aa93),
        (69_461, 0x64fc_acf9_b9d3_b8f7_9d42_27be_2c04_36d6),
    ],
    // ordinary velocity=true ssao=false receiver=Nearest
    [
        (96_658, 0x2e39_b2fc_daf3_2647_737c_48d6_7ad4_2640),
        (48_485, 0xafc7_ed68_be55_d5ab_299c_ec54_9f02_3489),
        (50_896, 0x2b6e_1cb6_e826_b30e_2b7f_11b0_dc97_bdba),
        (56_292, 0x3391_3465_2e52_bed4_6e8a_9b02_de6a_1e17),
        (62_615, 0x0b78_c1ae_a132_ad5c_15f0_2f5d_ce90_4e2c),
        (68_011, 0xd0e4_965d_96c3_9972_ae0d_52a4_bfca_e86d),
    ],
    // ordinary velocity=true ssao=true receiver=Nearest
    [
        (97_077, 0x1c0b_921a_9056_7bc4_eade_21fc_4734_32de),
        (48_904, 0xd5c5_502e_2f29_f5f6_1416_a392_22e2_fbf3),
        (51_315, 0x7af2_c3bc_1567_493b_5f93_a9b5_a489_952e),
        (56_711, 0x1712_f69d_3303_2241_0f56_e474_cb5f_ead1),
        (63_034, 0x1180_650a_489c_35b9_fccf_5cd0_2a27_f5f6),
        (68_430, 0xe72c_f32f_e421_5f09_8b60_029c_99ce_be09),
    ],
    // ordinary velocity=true ssao=true receiver=DepthAwareGather
    [
        (98_471, 0x0e36_8ebc_b4a3_bbe5_068e_2f6e_15d3_a66d),
        (50_298, 0x703d_e43f_cff8_987a_13bf_d3b8_f2c6_8af0),
        (52_709, 0x2679_de4b_8752_503b_5089_189f_4d3f_77a9),
        (58_105, 0x8108_7cff_0407_7c6b_8e53_af25_ecf8_c144),
        (64_428, 0xeac8_3bf2_9b55_89ce_ec06_9139_e4b4_7651),
        (69_824, 0x9c14_c8f8_2582_7bfa_2990_ee2c_e167_d9a4),
    ],
    // receiver ssao=false reflection=false velocity=false receiver=Nearest
    [
        (105_825, 0x7808_77e5_02c6_db5c_2eaf_ce2d_c15d_192c),
        (61_323, 0x8db4_3d7d_4395_a68e_16dd_8dc3_1045_6365),
        (63_734, 0x8269_cec5_dd43_a6fc_3ed7_7ac7_3ae7_1092),
        (69_130, 0x1687_33b6_311a_860d_5776_7778_9bb0_215b),
        (75_453, 0x5251_144d_9acc_0dc6_1b4f_7f83_f486_af50),
        (80_849, 0x37b3_2f75_e74a_bdd5_6de4_aec0_0054_d4c5),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (109_859, 0xddf9_6279_58ae_790b_0286_5c7a_ce29_7f22),
        (61_686, 0x0ea6_b8f1_b437_7764_3adc_fd7c_c557_556f),
        (64_097, 0x1311_80c1_3bd1_215c_2321_0fee_0b0a_15ee),
        (69_493, 0xbe44_cd78_6fb2_fe53_8098_a484_0211_73cb),
        (75_816, 0xe9bb_8b4f_3a9c_336a_cd91_0b53_6939_7fa0),
        (81_212, 0x8e83_8c3c_d4d2_57e2_b1a2_d007_c1ef_2b11),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (109_347, 0x6be5_58d2_68cf_3e7d_4986_d06e_d9ef_dc8a),
        (63_787, 0xeaad_704f_23e2_2b26_0bbc_6165_e6b8_affe),
        (66_198, 0x45b0_08c7_5b56_9bbd_749c_0eac_b218_b8f9),
        (71_594, 0x3b1c_849f_2a4f_eaa1_d37d_77d4_74c9_4b30),
        (77_917, 0x7a67_5e98_40e7_0dc3_60f4_fd0d_626e_aad1),
        (83_313, 0x21ce_417b_e2a6_4e16_20b5_d21d_03fd_8af0),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (110_741, 0xa804_6154_8118_3d14_17bc_26dd_3640_b015),
        (65_181, 0xaf39_3c80_0866_c971_1194_7c90_0fe4_ef39),
        (67_592, 0x470d_84f6_f124_7746_31ee_a0c0_b0da_59d2),
        (72_988, 0x6267_f1b3_0145_c29c_075e_0492_b618_25d9),
        (79_311, 0xb873_b7bf_c5b9_3967_17d3_6a8d_5a71_336a),
        (84_707, 0x7077_4bce_bd67_0eab_1724_8a3d_3e74_ae81),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (113_695, 0x1757_947b_a5f2_3f78_10e3_dde7_6685_5aa8),
        (64_144, 0x75f9_58f7_a9b4_32da_6b54_73b6_435b_563b),
        (66_555, 0xbd75_c01f_f6fd_39e9_8398_4c6b_0a61_ae5a),
        (71_951, 0x98df_b2c5_80b3_39a2_7811_fe35_5c50_0935),
        (78_274, 0x8e91_fa6a_20c4_5b21_2c21_4b5d_e70c_e752),
        (83_670, 0x7732_e164_249f_9858_a6bc_8508_4d15_6a7d),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (115_089, 0xed0a_de05_cf96_5292_e262_137c_0c0b_5f3d),
        (65_538, 0xb1d1_b7cb_3144_4943_6553_b64a_c332_6a5e),
        (67_949, 0x18f9_6164_9750_ecb4_2fe6_7422_40ac_c097),
        (73_345, 0x7b55_c8ab_94b3_7ab5_b864_5600_6993_cafa),
        (79_668, 0x530b_8904_743f_e585_28cf_6627_94c9_07cf),
        (85_064, 0xee6b_3c66_b666_1354_eafc_00e5_a381_f06a),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (106_244, 0x27c5_73a2_6342_8c93_86ef_372b_cb6e_d56c),
        (61_742, 0xbf26_30a9_06ea_2a90_ad18_a0ca_82ea_b937),
        (64_153, 0xda56_2e91_b2c0_142e_7f5a_c120_2e7e_6f36),
        (69_549, 0x1142_3d9c_681c_cab8_8181_22bc_19fb_f75d),
        (75_872, 0xb6c4_9c09_9a67_c1cb_9688_3b52_5ed3_f506),
        (81_268, 0x4c5b_02b9_3cc1_9c09_85af_4936_23e2_021d),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (107_638, 0xbe0f_36df_f460_140c_198f_4331_9f1a_42d3),
        (63_136, 0xe49c_dd57_ca5e_52d7_bc77_2629_2ac1_0258),
        (65_547, 0xc456_e96a_4cf6_7b60_bbae_cf06_75a8_cf65),
        (70_943, 0x6fa7_8dd7_d540_0aa1_de4f_dae6_2e85_f154),
        (77_266, 0x683d_03a3_313b_c760_68e9_c0e4_2b65_8b55),
        (82_662, 0xb0f7_2ea8_5baa_e8df_eda3_405f_965e_709c),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (110_278, 0x0602_f39e_e6ec_b1d3_041e_88e8_a273_fc76),
        (62_105, 0x4eed_021f_8aa2_83b0_71b2_832e_54e4_839b),
        (64_516, 0x5f6a_f11c_ed87_caa4_59b4_a11c_1859_6ee4),
        (69_912, 0x44c0_8bd8_46f2_ffd8_8328_19f0_23de_6113),
        (76_235, 0x6ebe_e9ef_1d91_048d_eaaf_49d1_daac_fb34),
        (81_631, 0x4652_c1c7_e9a3_d0fd_eaaa_bc53_d637_eac3),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (111_672, 0x3901_ef28_8b40_7d88_376c_193c_7f53_3d01),
        (63_499, 0xdb86_9f87_1337_207d_7cf7_de78_543f_b02c),
        (65_910, 0x2dc2_65ad_0025_3e63_6246_0235_ca80_4827),
        (71_306, 0x6d5c_90bb_c94d_fa79_7876_3b81_7efb_6fbe),
        (77_629, 0xdba0_2260_39ac_f48a_bb8b_e80f_59c1_b92f),
        (83_025, 0xa342_9b92_9312_5614_0eaf_64b6_9415_21fe),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (109_347, 0x6be5_58d2_68cf_3e7d_4986_d06e_d9ef_dc8a),
        (63_787, 0xeaad_704f_23e2_2b26_0bbc_6165_e6b8_affe),
        (66_198, 0x45b0_08c7_5b56_9bbd_749c_0eac_b218_b8f9),
        (71_594, 0x3b1c_849f_2a4f_eaa1_d37d_77d4_74c9_4b30),
        (77_917, 0x7a67_5e98_40e7_0dc3_60f4_fd0d_626e_aad1),
        (83_313, 0x21ce_417b_e2a6_4e16_20b5_d21d_03fd_8af0),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (110_741, 0xa804_6154_8118_3d14_17bc_26dd_3640_b015),
        (65_181, 0xaf39_3c80_0866_c971_1194_7c90_0fe4_ef39),
        (67_592, 0x470d_84f6_f124_7746_31ee_a0c0_b0da_59d2),
        (72_988, 0x6267_f1b3_0145_c29c_075e_0492_b618_25d9),
        (79_311, 0xb873_b7bf_c5b9_3967_17d3_6a8d_5a71_336a),
        (84_707, 0x7077_4bce_bd67_0eab_1724_8a3d_3e74_ae81),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (113_695, 0x1757_947b_a5f2_3f78_10e3_dde7_6685_5aa8),
        (64_144, 0x75f9_58f7_a9b4_32da_6b54_73b6_435b_563b),
        (66_555, 0xbd75_c01f_f6fd_39e9_8398_4c6b_0a61_ae5a),
        (71_951, 0x98df_b2c5_80b3_39a2_7811_fe35_5c50_0935),
        (78_274, 0x8e91_fa6a_20c4_5b21_2c21_4b5d_e70c_e752),
        (83_670, 0x7732_e164_249f_9858_a6bc_8508_4d15_6a7d),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (115_089, 0xed0a_de05_cf96_5292_e262_137c_0c0b_5f3d),
        (65_538, 0xb1d1_b7cb_3144_4943_6553_b64a_c332_6a5e),
        (67_949, 0x18f9_6164_9750_ecb4_2fe6_7422_40ac_c097),
        (73_345, 0x7b55_c8ab_94b3_7ab5_b864_5600_6993_cafa),
        (79_668, 0x530b_8904_743f_e585_28cf_6627_94c9_07cf),
        (85_064, 0xee6b_3c66_b666_1354_eafc_00e5_a381_f06a),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=Nearest
    [
        (96_146, 0xc5ab_532a_49dc_9cd0_03d3_010c_f7d3_74b4),
        (50_586, 0xef34_2a92_00cc_f9ee_b631_0b3f_49bd_c5b2),
        (52_997, 0x5b11_e143_9f09_b295_0b45_a437_f830_aca7),
        (58_393, 0xc80a_5332_3ca1_fdbb_f4c7_968b_236b_4cda),
        (64_716, 0x62ca_63dd_1fad_8d46_fdee_06a6_396c_020f),
        (70_112, 0x536d_603b_fa97_e8e3_fda4_19c3_0690_c46a),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=DepthAwareGather
    [
        (97_540, 0x8c57_8d2d_af4f_5716_fa40_e01d_9d3c_1457),
        (51_980, 0x7344_75d6_5115_43e6_c738_41b9_f606_6dd1),
        (54_391, 0x4ab5_b6b3_3fa6_07ba_614f_5234_e864_02b0),
        (59_787, 0xbc55_ffdc_6ab3_05f2_a77f_bb7f_683c_7d83),
        (66_110, 0xa243_b3e2_d2f3_9964_2b2c_3014_0a2b_6d30),
        (71_506, 0x642a_768a_2144_bf2b_6947_96d9_9267_4c33),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=Nearest
    [
        (96_212, 0x34e7_c8d2_19d5_7fe4_0858_a4f1_8dcf_2295),
        (50_652, 0xc671_411b_5c70_1eae_3d1a_6cb2_d87d_f1eb),
        (53_063, 0x2498_5381_2c5e_b962_e6fe_e5ef_f51f_8ade),
        (58_459, 0x999d_a48b_6f11_dcbd_72fa_4d7d_5453_b035),
        (64_782, 0x5049_8fe6_d933_a046_c28b_b5dc_d21b_370e),
        (70_178, 0x8fef_bd09_7502_7e9b_672c_4ef2_0464_0075),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=DepthAwareGather
    [
        (97_606, 0xa135_3dac_39be_a396_daf6_8616_9d3d_c532),
        (52_046, 0x22aa_756d_26c2_5eb3_ebae_1e56_418a_b1f0),
        (54_457, 0x7894_8029_a69a_360f_2638_b80a_dc8e_7d59),
        (59_853, 0x6e2a_a5ff_c6b7_7ea3_315b_13c5_3bc4_0c80),
        (66_176, 0xd573_1798_968e_62f0_da7d_4346_96b2_92d9),
        (71_572, 0xd6de_c6cb_3b9c_f828_59a3_2eed_991b_1e98),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=Nearest
    [
        (100_494, 0x2257_c26e_6f81_e231_182b_1c05_f444_d53a),
        (50_943, 0xb6c5_909e_4e54_447a_4279_654e_ec6c_430f),
        (53_354, 0xdea9_6d85_8199_4e04_16c9_3b36_a217_b1d8),
        (58_750, 0x06b6_489b_d98d_8784_8c04_b2e2_6f63_067f),
        (65_073, 0x34f0_d8ce_f678_6de2_85f0_af74_4ab8_cb68),
        (70_469, 0x1923_1f37_2db4_aa04_70ce_fec2_99b5_d54f),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=DepthAwareGather
    [
        (101_888, 0xda44_6a90_e324_005b_39d5_5cac_7d67_74e7),
        (52_337, 0xced3_bda5_bdda_cc42_3579_3433_f6de_3436),
        (54_748, 0xc835_3c4d_35be_67b4_6deb_a4d3_ce38_a66d),
        (60_144, 0x59c2_0b7b_a1a2_b537_581f_5ee5_5963_16a4),
        (66_467, 0x6c2a_6bb0_f652_8568_294e_8efe_e92e_76a5),
        (71_863, 0x08bb_7388_3e5e_db4a_c949_bf08_774a_77f4),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=Nearest
    [
        (100_560, 0xbcfe_8050_3ce2_cfd8_b288_eb5a_3266_a359),
        (51_009, 0xad77_0531_f20d_c470_8ab4_2991_c75e_f4c2),
        (53_420, 0x2808_f259_329b_632c_c6ca_e0ad_1a9f_d385),
        (58_816, 0x9d8d_57e7_988f_db2e_965a_102a_65cf_4988),
        (65_139, 0xc775_5e3b_c4f9_396e_d1a3_199d_0fb0_df15),
        (70_535, 0x6711_0161_1d79_2992_007d_0924_2c12_1350),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=DepthAwareGather
    [
        (101_954, 0xdf47_3304_bba4_b65e_a899_9a5c_2c07_bc50),
        (52_403, 0x19a7_5eb2_5a5d_bef1_b976_43a8_3a91_326b),
        (54_814, 0x9e33_712c_dbba_859d_6891_0d65_da88_11e0),
        (60_210, 0xa3bd_4509_6c07_161f_e044_20c9_e2e0_0773),
        (66_533, 0xd37a_4408_7c0e_acf4_f985_3a25_01af_8e38),
        (71_929, 0x5042_41fe_e500_8ef5_bbee_aa3c_bcd7_0fdb),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=Nearest
    [
        (109_347, 0x6be5_58d2_68cf_3e7d_4986_d06e_d9ef_dc8a),
        (63_787, 0xeaad_704f_23e2_2b26_0bbc_6165_e6b8_affe),
        (66_198, 0x45b0_08c7_5b56_9bbd_749c_0eac_b218_b8f9),
        (71_594, 0x3b1c_849f_2a4f_eaa1_d37d_77d4_74c9_4b30),
        (77_917, 0x7a67_5e98_40e7_0dc3_60f4_fd0d_626e_aad1),
        (83_313, 0x21ce_417b_e2a6_4e16_20b5_d21d_03fd_8af0),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (110_741, 0xa804_6154_8118_3d14_17bc_26dd_3640_b015),
        (65_181, 0xaf39_3c80_0866_c971_1194_7c90_0fe4_ef39),
        (67_592, 0x470d_84f6_f124_7746_31ee_a0c0_b0da_59d2),
        (72_988, 0x6267_f1b3_0145_c29c_075e_0492_b618_25d9),
        (79_311, 0xb873_b7bf_c5b9_3967_17d3_6a8d_5a71_336a),
        (84_707, 0x7077_4bce_bd67_0eab_1724_8a3d_3e74_ae81),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (109_413, 0x43d4_fdcc_e1b7_9d4d_3668_2a9b_0c5e_cafb),
        (63_853, 0x5b25_5e39_adf2_995e_30c2_233a_e78f_284f),
        (66_264, 0x4309_1a54_342f_492a_0ed1_b817_fd08_c7f8),
        (71_660, 0x1f83_d44a_6612_7518_a3e5_5416_a9a0_ef1b),
        (77_983, 0xf7a1_1030_09cc_5f7a_85bc_0d99_0086_4890),
        (83_379, 0xf553_d5c1_6703_1bd2_e715_200b_949c_ac93),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (110_807, 0xbd99_0dd2_bba5_a08f_9f6a_edcb_9f56_cc80),
        (65_247, 0xc71d_32b0_853d_29a1_60a1_c73d_d63d_ed08),
        (67_658, 0xbed8_7bf7_4340_c25f_b0da_0967_1ee2_12db),
        (73_054, 0x5b33_f049_4956_44a6_57fa_3a95_3442_52ee),
        (79_377, 0xbe8a_67ce_cf15_5dd1_9949_7a6b_f686_ddfb),
        (84_773, 0x4a95_26da_f2c4_92ac_fc96_300f_b78d_04a6),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (113_695, 0x1757_947b_a5f2_3f78_10e3_dde7_6685_5aa8),
        (64_144, 0x75f9_58f7_a9b4_32da_6b54_73b6_435b_563b),
        (66_555, 0xbd75_c01f_f6fd_39e9_8398_4c6b_0a61_ae5a),
        (71_951, 0x98df_b2c5_80b3_39a2_7811_fe35_5c50_0935),
        (78_274, 0x8e91_fa6a_20c4_5b21_2c21_4b5d_e70c_e752),
        (83_670, 0x7732_e164_249f_9858_a6bc_8508_4d15_6a7d),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (115_089, 0xed0a_de05_cf96_5292_e262_137c_0c0b_5f3d),
        (65_538, 0xb1d1_b7cb_3144_4943_6553_b64a_c332_6a5e),
        (67_949, 0x18f9_6164_9750_ecb4_2fe6_7422_40ac_c097),
        (73_345, 0x7b55_c8ab_94b3_7ab5_b864_5600_6993_cafa),
        (79_668, 0x530b_8904_743f_e585_28cf_6627_94c9_07cf),
        (85_064, 0xee6b_3c66_b666_1354_eafc_00e5_a381_f06a),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (113_761, 0x6d93_06e2_8401_7f3f_13c3_7c78_4bbb_fd97),
        (64_210, 0xdd9b_b94e_131e_8e23_d405_c2dc_a51a_9586),
        (66_621, 0x53d4_9b7d_eeb8_ce7c_377c_9a1a_7940_fcdf),
        (72_017, 0x5961_7a49_371a_394d_32bf_dd9c_a40f_c99e),
        (78_340, 0xe7eb_34c0_2c29_6f40_a7c9_94ad_232c_e51f),
        (83_736, 0x7478_6d87_0b21_6e78_cf9d_79e0_b573_9636),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (115_155, 0x3886_ac10_5aea_0f26_f8a5_a67e_e7b7_ba9e),
        (65_604, 0x4f47_809b_491e_4abb_5406_5078_906f_d103),
        (68_015, 0x4bfa_d21b_8c20_3781_db8c_4ee8_b1ec_bf72),
        (73_411, 0x65b6_a9ad_676d_f9f6_0ab4_cbf6_af5a_c8e9),
        (79_734, 0xbaf9_20b0_9563_2e2e_cc64_c9c8_a000_00a2),
        (85_130, 0x97ce_e029_2555_2854_4a1b_c32c_4f48_dce9),
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
    (92_613, 0xdb10_cc54_7145_1b02_22e4_f07c_7455_f5ae),
    (93_032, 0xed11_b6ec_7e80_e8c9_cf71_a55b_76c8_22aa),
    (94_426, 0x875f_db96_8909_00b7_6bb0_7387_7191_53b9),
    (96_647, 0x0eb6_e282_e118_8215_b925_90e3_05eb_9d94),
    (97_066, 0x0ccd_b7ff_d6e4_ccd4_2e82_4c86_fada_8424),
    (98_460, 0x5295_727e_50cd_599f_2de6_da99_4820_f037),
    (105_814, 0x09ba_62a1_7be3_6d3c_1a20_2ca1_d93d_c0ac),
    (109_848, 0x6f4b_92c5_3ffb_1f1c_aeaf_dfde_1081_ceb2),
    (109_336, 0x1523_9a22_7cf3_a26a_c011_979f_eb2b_39a2),
    (110_730, 0x80e6_5792_fc27_d5b1_52c6_865b_c1e2_1df9),
    (113_684, 0xaa32_59f4_f310_09ee_79f2_5e43_824f_f8c0),
    (115_078, 0x18cd_710b_0f11_c426_4bcd_2b18_b027_3889),
    (106_233, 0x46ae_6b4d_fc6c_1a78_7bd7_650e_fee5_b162),
    (107_627, 0xcafe_2dcb_521d_4a0c_b3bf_36b5_e5f4_ae6d),
    (110_267, 0x86b5_c51c_aba5_fc93_73d4_3743_d29e_00cc),
    (111_661, 0x1b22_b298_ac1e_828c_b0e7_4799_957e_0573),
    (109_336, 0x1523_9a22_7cf3_a26a_c011_979f_eb2b_39a2),
    (110_730, 0x80e6_5792_fc27_d5b1_52c6_865b_c1e2_1df9),
    (113_684, 0xaa32_59f4_f310_09ee_79f2_5e43_824f_f8c0),
    (115_078, 0x18cd_710b_0f11_c426_4bcd_2b18_b027_3889),
    (96_135, 0xc968_1b75_5ee0_4ff3_cc0e_2db8_c26e_ee08),
    (97_529, 0x2a2b_6049_acf4_616e_6598_8462_2f76_e70f),
    (96_201, 0x31a8_4e27_2d66_8c12_d3da_b7be_8c16_2161),
    (97_595, 0x016e_372c_7487_7008_5817_aa19_f450_32da),
    (100_483, 0x59ae_5f69_d1aa_51b0_104d_4707_4c0d_3006),
    (101_877, 0xa94b_5d5b_efab_7344_c909_f0ce_9fa7_993f),
    (100_549, 0x47d9_f29f_4ba6_9992_2be5_5d57_223c_f58d),
    (101_943, 0x7b62_c0a7_f58d_4896_0070_8354_d687_e2f8),
    (109_336, 0x1523_9a22_7cf3_a26a_c011_979f_eb2b_39a2),
    (110_730, 0x80e6_5792_fc27_d5b1_52c6_865b_c1e2_1df9),
    (109_402, 0xc531_b64b_5c87_b333_6ab8_9966_c3dd_c383),
    (110_796, 0x7028_0714_61e7_6de4_bb0c_250b_2be4_3c4c),
    (113_684, 0xaa32_59f4_f310_09ee_79f2_5e43_824f_f8c0),
    (115_078, 0x18cd_710b_0f11_c426_4bcd_2b18_b027_3889),
    (113_750, 0x54f0_b7a1_74a2_ae58_67d3_ac8a_b2f0_de1f),
    (115_144, 0x4620_77ee_d2de_74d5_57fb_c592_28e6_0b82),
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
