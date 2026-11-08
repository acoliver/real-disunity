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
        (107_405, 0xe5b4_1bd6_3bda_3365_b7cf_0ad2_881e_7410),
        (62_903, 0xd1f2_3665_ef6a_e710_01dc_258d_3580_1d51),
        (65_314, 0x9649_201f_568c_b356_0614_843b_cd44_9db6),
        (70_710, 0x5b85_e86b_5fe5_fc94_b822_8c1b_2518_3bcf),
        (77_033, 0xd64c_0d8f_e15e_5fb9_331a_6e1e_633b_3fb4),
        (82_429, 0x1e92_d134_7178_d12e_2b0f_185e_c934_c7f9),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (111_439, 0xa676_c62c_2d4f_12a8_1a07_fea4_e9f5_c6ce),
        (63_266, 0xd947_7563_21d7_bd8c_57c2_0f91_efd9_df63),
        (65_677, 0x8cbc_73d9_e0ab_5235_81d4_f6b8_f2c9_acda),
        (71_073, 0xf0e1_69ca_8d9b_6518_b4c1_adf1_4074_2537),
        (77_396, 0xfaf7_1347_90ed_1f9a_6281_056e_fefb_fb8c),
        (82_792, 0x82b6_8176_f9ba_aeba_5169_43fc_9e83_53cd),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (110_927, 0x4d62_dce1_cf11_54db_f903_de18_20cb_9fde),
        (65_367, 0x8c76_0e86_1c9b_5a05_2620_5736_d638_e2de),
        (67_778, 0x695a_1a9f_d726_6209_0bcf_b77a_855d_d229),
        (73_174, 0xa3b3_ac42_f88e_2374_1ff8_054d_78d8_6e60),
        (79_497, 0x0bde_154e_1a47_fb32_9cb6_e991_376a_05a1),
        (84_893, 0x2572_df99_e593_42a1_6e38_0762_1bce_acc0),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (112_321, 0x8098_f8e2_48c8_c4dd_8016_1387_dd85_5301),
        (66_761, 0x1f90_5afd_d5ea_ff83_5463_dbf0_9db2_f90d),
        (69_172, 0xa29a_9a77_61f1_d670_f78d_1318_5da9_9a1e),
        (74_568, 0xd900_dbb8_cda5_0150_f42e_aa2f_697a_d275),
        (80_891, 0x148c_2e60_90ef_3d76_4d93_8019_5b4c_eec6),
        (86_287, 0xa14d_2455_ffc8_72b9_d5f9_721b_7f3e_497d),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (115_275, 0x181e_d173_27f5_721e_1d95_8801_231c_2dc4),
        (65_724, 0xcd6a_f24c_710e_ac30_6a10_5192_e516_a15b),
        (68_135, 0x08fe_fdf4_36f8_e39d_7621_93a9_5452_dcaa),
        (73_531, 0xa24e_15db_b5b7_32c4_29a4_7a41_7268_f205),
        (79_854, 0x02b8_6be4_1de7_63c5_1487_132c_fecc_3602),
        (85_250, 0xe30d_d2ac_a823_1cb9_5cd5_95bf_71c1_7e2d),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (116_669, 0x9f68_42a9_8044_0517_8abf_8fb8_ddff_80c1),
        (67_118, 0x3446_3a89_28e2_9c41_b30d_f309_e668_71e2),
        (69_529, 0x092d_4032_4943_b20f_a23b_0327_e93b_23d3),
        (74_925, 0xbb1b_6edb_12df_af1f_b51e_4199_c0b1_6726),
        (81_248, 0x2838_e89d_9a23_c475_d324_c331_1960_912b),
        (86_644, 0x8f51_3241_d440_2e19_353b_e226_f447_0c36),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (107_824, 0xbaf7_73f4_37a7_a7fc_4749_d807_0022_abbc),
        (63_322, 0x02b7_c224_c683_48bc_28d7_e186_9217_6247),
        (65_733, 0x99f9_c097_9ba4_ec0e_9105_b2ef_8046_92fe),
        (71_129, 0x401d_44f0_ed62_c72f_4d8e_eeb2_dcb4_57f5),
        (77_452, 0x7360_7ccf_5847_7276_9ac3_1fcb_9036_c8fe),
        (82_848, 0xa667_f15c_945f_5418_81d5_5f68_53fe_f485),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (109_218, 0x40f7_ddf9_bc4a_e2ca_5038_b655_52fe_f9d7),
        (64_716, 0xc89e_0695_0228_30b2_7183_201e_ded3_7844),
        (67_127, 0x1104_8238_a807_de71_4aa3_78d9_22ac_33e9),
        (72_523, 0x4517_1004_b9dd_0aa6_cf35_473e_8227_9558),
        (78_846, 0xe46b_f534_f8f9_9362_8ec0_05c1_e988_6579),
        (84_242, 0x824e_d060_3f96_9bae_dfaa_bef0_b04c_3940),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (111_858, 0x8731_3993_e35f_228e_5d99_9f1a_aede_3fa6),
        (63_685, 0xe496_1d72_02b7_224a_e038_dc8b_e76f_0d6b),
        (66_096, 0x49d3_fb4b_c664_c596_b46e_5b5e_fa72_c7cc),
        (71_492, 0xe5f0_8977_3a12_455c_afbe_2c51_b50f_f7ab),
        (77_815, 0x16ed_dd5c_3636_e8dc_da42_bca9_de75_996c),
        (83_211, 0xbdf3_7bc4_10e6_411a_0dc5_1300_923d_b76b),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (113_252, 0xd17f_5bed_4930_b22d_ed91_0763_463f_18cd),
        (65_079, 0xcb54_3809_d59a_b508_50e0_bfa3_56fb_9750),
        (67_490, 0xb88d_8902_79ea_5e9e_91ff_137e_7d7b_bcc3),
        (72_886, 0x66aa_aeb6_1096_035e_cd47_9748_a51b_9f6a),
        (79_209, 0xce47_382b_1fbc_a248_6f4b_da0b_c84c_fdab),
        (84_605, 0x3552_f7fb_9a39_4166_759f_27a0_4f06_baca),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (110_927, 0x4d62_dce1_cf11_54db_f903_de18_20cb_9fde),
        (65_367, 0x8c76_0e86_1c9b_5a05_2620_5736_d638_e2de),
        (67_778, 0x695a_1a9f_d726_6209_0bcf_b77a_855d_d229),
        (73_174, 0xa3b3_ac42_f88e_2374_1ff8_054d_78d8_6e60),
        (79_497, 0x0bde_154e_1a47_fb32_9cb6_e991_376a_05a1),
        (84_893, 0x2572_df99_e593_42a1_6e38_0762_1bce_acc0),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (112_321, 0x8098_f8e2_48c8_c4dd_8016_1387_dd85_5301),
        (66_761, 0x1f90_5afd_d5ea_ff83_5463_dbf0_9db2_f90d),
        (69_172, 0xa29a_9a77_61f1_d670_f78d_1318_5da9_9a1e),
        (74_568, 0xd900_dbb8_cda5_0150_f42e_aa2f_697a_d275),
        (80_891, 0x148c_2e60_90ef_3d76_4d93_8019_5b4c_eec6),
        (86_287, 0xa14d_2455_ffc8_72b9_d5f9_721b_7f3e_497d),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (115_275, 0x181e_d173_27f5_721e_1d95_8801_231c_2dc4),
        (65_724, 0xcd6a_f24c_710e_ac30_6a10_5192_e516_a15b),
        (68_135, 0x08fe_fdf4_36f8_e39d_7621_93a9_5452_dcaa),
        (73_531, 0xa24e_15db_b5b7_32c4_29a4_7a41_7268_f205),
        (79_854, 0x02b8_6be4_1de7_63c5_1487_132c_fecc_3602),
        (85_250, 0xe30d_d2ac_a823_1cb9_5cd5_95bf_71c1_7e2d),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (116_669, 0x9f68_42a9_8044_0517_8abf_8fb8_ddff_80c1),
        (67_118, 0x3446_3a89_28e2_9c41_b30d_f309_e668_71e2),
        (69_529, 0x092d_4032_4943_b20f_a23b_0327_e93b_23d3),
        (74_925, 0xbb1b_6edb_12df_af1f_b51e_4199_c0b1_6726),
        (81_248, 0x2838_e89d_9a23_c475_d324_c331_1960_912b),
        (86_644, 0x8f51_3241_d440_2e19_353b_e226_f447_0c36),
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
        (110_927, 0x4d62_dce1_cf11_54db_f903_de18_20cb_9fde),
        (65_367, 0x8c76_0e86_1c9b_5a05_2620_5736_d638_e2de),
        (67_778, 0x695a_1a9f_d726_6209_0bcf_b77a_855d_d229),
        (73_174, 0xa3b3_ac42_f88e_2374_1ff8_054d_78d8_6e60),
        (79_497, 0x0bde_154e_1a47_fb32_9cb6_e991_376a_05a1),
        (84_893, 0x2572_df99_e593_42a1_6e38_0762_1bce_acc0),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (112_321, 0x8098_f8e2_48c8_c4dd_8016_1387_dd85_5301),
        (66_761, 0x1f90_5afd_d5ea_ff83_5463_dbf0_9db2_f90d),
        (69_172, 0xa29a_9a77_61f1_d670_f78d_1318_5da9_9a1e),
        (74_568, 0xd900_dbb8_cda5_0150_f42e_aa2f_697a_d275),
        (80_891, 0x148c_2e60_90ef_3d76_4d93_8019_5b4c_eec6),
        (86_287, 0xa14d_2455_ffc8_72b9_d5f9_721b_7f3e_497d),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (110_993, 0xff79_27d0_12a2_c99f_28b7_97e0_e8d9_4d47),
        (65_433, 0xe771_63da_9a0f_325e_e042_b72c_9120_6f2f),
        (67_844, 0xcca6_d70e_a521_457b_c7ea_5539_d435_9c68),
        (73_240, 0x98bb_04c2_d183_9446_b77d_e33d_0295_350b),
        (79_563, 0x43a1_19cd_2437_efc4_b2ab_76ec_3176_59e0),
        (84_959, 0x8ae7_791f_2204_c915_2be6_bd7c_538e_3123),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (112_387, 0xfe41_5daa_ad57_403b_7518_255b_d5c0_2504),
        (66_827, 0xd0fd_aa0b_7868_c709_a7e9_16a2_6c36_1ed4),
        (69_238, 0x2b36_6f54_58b7_cb87_0f8c_e63d_ef20_5e7f),
        (74_634, 0x813f_c4ad_afa7_df24_f47e_5988_430a_9772),
        (80_957, 0x3329_6af8_3a1f_8759_08fb_f4bf_3477_ba9f),
        (86_353, 0xe6bc_171a_8ff6_b248_aa02_cf67_3d48_910a),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (115_275, 0x181e_d173_27f5_721e_1d95_8801_231c_2dc4),
        (65_724, 0xcd6a_f24c_710e_ac30_6a10_5192_e516_a15b),
        (68_135, 0x08fe_fdf4_36f8_e39d_7621_93a9_5452_dcaa),
        (73_531, 0xa24e_15db_b5b7_32c4_29a4_7a41_7268_f205),
        (79_854, 0x02b8_6be4_1de7_63c5_1487_132c_fecc_3602),
        (85_250, 0xe30d_d2ac_a823_1cb9_5cd5_95bf_71c1_7e2d),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (116_669, 0x9f68_42a9_8044_0517_8abf_8fb8_ddff_80c1),
        (67_118, 0x3446_3a89_28e2_9c41_b30d_f309_e668_71e2),
        (69_529, 0x092d_4032_4943_b20f_a23b_0327_e93b_23d3),
        (74_925, 0xbb1b_6edb_12df_af1f_b51e_4199_c0b1_6726),
        (81_248, 0x2838_e89d_9a23_c475_d324_c331_1960_912b),
        (86_644, 0x8f51_3241_d440_2e19_353b_e226_f447_0c36),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (115_341, 0x2da9_645a_48e1_1786_d77c_6476_6b1a_64bb),
        (65_790, 0x66c3_a572_9fb1_087d_359f_d024_e88c_33a6),
        (68_201, 0x81a1_2ee8_49d5_51e8_de73_14e0_22a7_52ef),
        (73_597, 0xddda_c690_d54a_d4fa_5c0d_1352_bf5b_caae),
        (79_920, 0x5f7c_b836_831c_85f1_a012_4d22_e17d_b20f),
        (85_316, 0x980a_971b_6dd9_86a1_e4de_bad9_85f9_ea66),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (116_735, 0x8492_9f6c_89d1_8328_945f_75b2_a9c4_d56a),
        (67_184, 0x5231_e309_5e31_01f5_c5f4_e3bc_d5f4_790f),
        (69_595, 0x4144_c126_b806_d9e5_fa74_dc6e_3d43_9ca6),
        (74_991, 0x2159_4156_9b2f_2458_97ab_284b_165b_138d),
        (81_314, 0xe321_2b21_59ac_c1cd_2af7_2696_645e_e0a6),
        (86_710, 0xdd35_736d_f16d_5c0c_8804_eb39_fb7b_d6ad),
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
    (107_394, 0x05fd_ea0b_45f5_d56f_1a4d_5390_869b_8a48),
    (111_428, 0x7c9f_80fa_e62e_babc_3007_1518_c2aa_69e6),
    (110_916, 0x7f32_ea69_a9e4_9b72_1baf_7f67_a4d6_08de),
    (112_310, 0x0597_29e9_2e49_de53_177c_ca07_c2b8_ae5d),
    (115_264, 0x9bcb_85ba_25c8_50e2_f3b0_516d_6605_3e74),
    (116_658, 0x45c4_1e47_a1ed_8b0f_6f11_4b03_a4dc_84f5),
    (107_813, 0x9c52_e377_b406_2595_6e73_bd59_ca46_ac72),
    (109_207, 0x6cad_b13f_499e_67e8_a385_34be_04a9_c5d9),
    (111_847, 0xf5f8_4761_e3b1_8228_13d2_f557_9279_eefc),
    (113_241, 0x6ca1_0f92_7eec_4056_5516_0577_52f5_80d7),
    (110_916, 0x7f32_ea69_a9e4_9b72_1baf_7f67_a4d6_08de),
    (112_310, 0x0597_29e9_2e49_de53_177c_ca07_c2b8_ae5d),
    (115_264, 0x9bcb_85ba_25c8_50e2_f3b0_516d_6605_3e74),
    (116_658, 0x45c4_1e47_a1ed_8b0f_6f11_4b03_a4dc_84f5),
    (96_993, 0x00a5_ee5b_9db2_a532_29c8_49b1_e3d3_ad7a),
    (98_387, 0x96c4_da16_ac82_20e4_528e_8688_8e92_4451),
    (97_059, 0xd587_eedf_51b3_d942_8eed_45f1_6289_406b),
    (98_453, 0x21c9_a714_8c85_e495_3807_8090_4834_9154),
    (101_341, 0x3431_f75c_619f_ada3_23b4_b908_840e_f5c8),
    (102_735, 0xf231_b2a0_6253_2ee4_9644_4489_49b8_0651),
    (101_407, 0x10cb_3513_a9cb_a365_45c3_c0d1_9b49_e6b7),
    (102_801, 0x17e2_a4b9_ddbc_4345_552d_293c_c3c7_7f3a),
    (110_916, 0x7f32_ea69_a9e4_9b72_1baf_7f67_a4d6_08de),
    (112_310, 0x0597_29e9_2e49_de53_177c_ca07_c2b8_ae5d),
    (110_982, 0xb6e2_7014_f6c5_f5d1_3267_8cb1_d01c_0e47),
    (112_376, 0xb73a_9da3_ce15_7541_163c_a48b_8d49_0cd8),
    (115_264, 0x9bcb_85ba_25c8_50e2_f3b0_516d_6605_3e74),
    (116_658, 0x45c4_1e47_a1ed_8b0f_6f11_4b03_a4dc_84f5),
    (115_330, 0x2a3a_2821_11f7_2258_4b67_d54d_651e_b0eb),
    (116_724, 0xb3f9_291f_b0f3_e5cc_b66a_d75e_8477_6646),
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
