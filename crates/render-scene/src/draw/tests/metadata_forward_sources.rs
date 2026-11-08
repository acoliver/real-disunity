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
        (109_371, 0xb4d0_a58f_16bb_82c2_bfcd_6115_674f_bc2b),
        (64_869, 0xdb31_8164_dd59_1240_4592_cc2e_6f1a_adba),
        (67_280, 0xbe65_ad49_3f9f_555b_79f5_3fb0_b1fc_c691),
        (72_676, 0x0102_1a46_2387_ee92_796b_6509_be24_d258),
        (78_999, 0xcff9_2154_b06e_a1d6_77d3_7bdc_9d8e_8351),
        (84_395, 0x579d_e113_0a94_6ae5_e2fc_4dd5_b18f_1e40),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (113_405, 0x5eff_7820_df4a_a39c_4f5a_a086_3ae8_86e1),
        (65_232, 0x4ac6_dadb_5aea_0f72_32d1_f6e6_e8d9_0a56),
        (67_643, 0x38e7_6a91_8687_a498_e322_e84b_baf3_4d23),
        (73_039, 0xb134_d6c5_0864_3941_9423_41f8_e73f_0ce2),
        (79_362, 0xeee9_0930_fa98_2329_e289_eeac_bdbf_68cb),
        (84_758, 0x4ab9_b218_ccf8_b2a2_4133_dae8_2d24_c602),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (67_333, 0x9da1_b2c2_4876_348e_b148_c841_ab29_0e05),
        (69_744, 0x10ea_c75d_8e52_da73_f959_a259_24ad_f176),
        (75_140, 0xc0d5_4dec_4d93_038b_f9c2_8232_ddc3_2107),
        (81_463, 0x82ab_b023_39ea_9bf8_e4e3_9461_3ae5_947c),
        (86_859, 0x54ff_c590_a5f5_7bc8_c335_756e_8de7_5c59),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (68_727, 0xb57a_400c_b555_ba93_6b50_252a_c26e_3e6e),
        (71_138, 0xf009_102b_d6c7_2b8b_7d77_0d7c_d386_99d1),
        (76_534, 0x57a6_0ea0_2ebc_489c_9370_a068_c2ac_c65e),
        (82_857, 0x60bc_230e_90a4_9f79_1a24_8059_56cc_754b),
        (88_253, 0xff4c_57f3_0fa2_77d1_e5eb_8632_f2b4_ea0c),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (67_690, 0xc3e5_44c2_b5b6_4127_4425_9dd4_97fb_8b76),
        (70_101, 0x1c54_9832_48e2_1300_c27b_09e2_634e_9d13),
        (75_497, 0xc434_f9b7_61f7_aabe_c4bf_b952_b180_2e50),
        (81_820, 0x7f5e_2f44_dfcc_d916_ac7d_ab33_9853_dbf9),
        (87_216, 0x1de4_c5f3_7bd8_1ec7_72a9_36c1_336a_e39a),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (69_084, 0x5313_236e_9848_e9b2_946e_3498_3897_1043),
        (71_495, 0x6522_a18f_eb35_c84f_bfc2_5d1d_dca2_1836),
        (76_891, 0x1e24_31f0_e74a_3485_43c9_8550_fe84_574f),
        (83_214, 0x03d4_6507_fda4_5776_2d4a_abeb_aa3d_4b90),
        (88_610, 0xb7e5_af23_bb20_bdf0_8d8c_58e0_dd15_cefd),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (109_790, 0xd613_0cb8_e05d_19f2_45d1_09f3_8749_f19d),
        (65_288, 0x57fc_2250_8480_7ca8_5747_ed9b_6a3a_34aa),
        (67_699, 0xcc37_c74a_08f7_c1c8_ea40_e499_fa4e_e74f),
        (73_095, 0x32f7_6134_6e6b_a8aa_176d_2b63_f728_922c),
        (79_418, 0x64d4_cfbb_8b22_b48f_0673_1021_9deb_5a11),
        (84_814, 0x3f6a_e59e_1b18_d5c9_b20f_b7b2_848f_d1fa),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (111_184, 0xa087_2c85_d593_f066_f476_7596_0601_33ee),
        (66_682, 0xa48a_a4f8_1edf_407c_2376_8ebc_2b23_af11),
        (69_093, 0xbbb7_fb31_9a39_5bfc_12b8_b3b5_b8e7_fbf0),
        (74_489, 0xb208_bc7c_c868_288d_cdb5_d3ac_ac58_4891),
        (80_812, 0xe420_2834_30b2_0972_f20d_7cb2_566f_fd8e),
        (86_208, 0x38ef_7824_59d1_7c40_5a82_96d6_7101_8573),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (113_824, 0x2f5d_59db_d9ee_2792_00bc_8f83_9a8d_905f),
        (65_651, 0x8766_b263_a874_1cd0_925e_d15c_c9c4_2bc0),
        (68_062, 0x5b3c_3e40_42d7_cf7d_d4a3_d35c_b470_7633),
        (73_458, 0x83cd_ec2e_7937_0452_a6a7_938c_1ce4_0314),
        (79_781, 0x115f_cd9d_3b04_8336_4175_3050_5406_39c9),
        (85_177, 0xb221_6c98_3a5f_29fd_2c14_24d9_8fb1_143e),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (115_218, 0x0da6_b367_dada_3d1c_0144_084a_fda4_200c),
        (67_045, 0x8c8b_8213_3783_5875_457a_3668_29c4_290b),
        (69_456, 0x8d29_5f32_6a5a_90c7_84e4_223f_c4e0_9924),
        (74_852, 0x6363_e454_b2d8_8fc8_2720_cc85_1be3_6fe9),
        (81_175, 0xf9ee_eb6c_ae9c_8f0b_9fd8_de06_39bb_2e36),
        (86_571, 0x4e95_b328_d8b0_3efa_445d_744d_bdea_c807),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (67_333, 0x9da1_b2c2_4876_348e_b148_c841_ab29_0e05),
        (69_744, 0x10ea_c75d_8e52_da73_f959_a259_24ad_f176),
        (75_140, 0xc0d5_4dec_4d93_038b_f9c2_8232_ddc3_2107),
        (81_463, 0x82ab_b023_39ea_9bf8_e4e3_9461_3ae5_947c),
        (86_859, 0x54ff_c590_a5f5_7bc8_c335_756e_8de7_5c59),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (68_727, 0xb57a_400c_b555_ba93_6b50_252a_c26e_3e6e),
        (71_138, 0xf009_102b_d6c7_2b8b_7d77_0d7c_d386_99d1),
        (76_534, 0x57a6_0ea0_2ebc_489c_9370_a068_c2ac_c65e),
        (82_857, 0x60bc_230e_90a4_9f79_1a24_8059_56cc_754b),
        (88_253, 0xff4c_57f3_0fa2_77d1_e5eb_8632_f2b4_ea0c),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (67_690, 0xc3e5_44c2_b5b6_4127_4425_9dd4_97fb_8b76),
        (70_101, 0x1c54_9832_48e2_1300_c27b_09e2_634e_9d13),
        (75_497, 0xc434_f9b7_61f7_aabe_c4bf_b952_b180_2e50),
        (81_820, 0x7f5e_2f44_dfcc_d916_ac7d_ab33_9853_dbf9),
        (87_216, 0x1de4_c5f3_7bd8_1ec7_72a9_36c1_336a_e39a),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (69_084, 0x5313_236e_9848_e9b2_946e_3498_3897_1043),
        (71_495, 0x6522_a18f_eb35_c84f_bfc2_5d1d_dca2_1836),
        (76_891, 0x1e24_31f0_e74a_3485_43c9_8550_fe84_574f),
        (83_214, 0x03d4_6507_fda4_5776_2d4a_abeb_aa3d_4b90),
        (88_610, 0xb7e5_af23_bb20_bdf0_8d8c_58e0_dd15_cefd),
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
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (67_333, 0x9da1_b2c2_4876_348e_b148_c841_ab29_0e05),
        (69_744, 0x10ea_c75d_8e52_da73_f959_a259_24ad_f176),
        (75_140, 0xc0d5_4dec_4d93_038b_f9c2_8232_ddc3_2107),
        (81_463, 0x82ab_b023_39ea_9bf8_e4e3_9461_3ae5_947c),
        (86_859, 0x54ff_c590_a5f5_7bc8_c335_756e_8de7_5c59),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (68_727, 0xb57a_400c_b555_ba93_6b50_252a_c26e_3e6e),
        (71_138, 0xf009_102b_d6c7_2b8b_7d77_0d7c_d386_99d1),
        (76_534, 0x57a6_0ea0_2ebc_489c_9370_a068_c2ac_c65e),
        (82_857, 0x60bc_230e_90a4_9f79_1a24_8059_56cc_754b),
        (88_253, 0xff4c_57f3_0fa2_77d1_e5eb_8632_f2b4_ea0c),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (112_959, 0xd6ae_f592_dd4c_3a55_f464_498d_1a6d_ba68),
        (67_399, 0x2169_7d7e_08b3_c59b_7379_2693_1da6_526c),
        (69_810, 0x3ef8_6b41_01a9_6885_9597_fe4c_4c24_6f57),
        (75_206, 0x7bae_584a_3671_f524_bf23_6478_5ed0_c31c),
        (81_529, 0xfd8e_2f9d_96e9_2cd9_10ee_ee09_b99d_18bd),
        (86_925, 0x9180_40a3_a5f3_3163_2a79_1d1b_5342_e83e),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (114_353, 0x5f06_5d1f_a862_1bb2_ef37_47cc_a6c3_4b4f),
        (68_793, 0x62d7_fc90_fb05_3431_10d5_25bc_e8ef_54cf),
        (71_204, 0xabcd_3afa_ef1f_d895_8f7c_314c_cc1c_fdf0),
        (76_600, 0x1c06_1495_5bd8_9650_ebb5_2698_506e_7a91),
        (82_923, 0xeae9_9425_4dd3_cbe2_f23b_5e51_a370_3992),
        (88_319, 0xa1f9_5e93_9133_7e80_3dac_4309_399c_99c7),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (67_690, 0xc3e5_44c2_b5b6_4127_4425_9dd4_97fb_8b76),
        (70_101, 0x1c54_9832_48e2_1300_c27b_09e2_634e_9d13),
        (75_497, 0xc434_f9b7_61f7_aabe_c4bf_b952_b180_2e50),
        (81_820, 0x7f5e_2f44_dfcc_d916_ac7d_ab33_9853_dbf9),
        (87_216, 0x1de4_c5f3_7bd8_1ec7_72a9_36c1_336a_e39a),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (69_084, 0x5313_236e_9848_e9b2_946e_3498_3897_1043),
        (71_495, 0x6522_a18f_eb35_c84f_bfc2_5d1d_dca2_1836),
        (76_891, 0x1e24_31f0_e74a_3485_43c9_8550_fe84_574f),
        (83_214, 0x03d4_6507_fda4_5776_2d4a_abeb_aa3d_4b90),
        (88_610, 0xb7e5_af23_bb20_bdf0_8d8c_58e0_dd15_cefd),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (117_307, 0xa6b8_60f3_a03a_0be9_c128_b751_e72e_5b8c),
        (67_756, 0x00dd_86af_fe2f_4aa6_ba6f_32eb_8eb9_6c23),
        (70_167, 0x8c08_e058_ef86_3219_7272_3d59_8c7c_394e),
        (75_563, 0x36c8_b4ee_5add_82e7_b0c4_13ec_b67a_0aaf),
        (81_886, 0x7d7f_6288_a865_90d5_f052_507b_ef77_41dc),
        (87_282, 0x8fce_917b_333f_90d2_3510_9ee8_6886_afd1),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (118_701, 0x263a_0b8b_cc5e_bf0d_7e60_79e8_d348_4be9),
        (69_150, 0x0d47_3f55_6098_bbb6_d6fb_432c_5225_3d16),
        (71_561, 0xcd49_e627_3044_796a_4e28_a0eb_c34c_766b),
        (76_957, 0x9d85_7811_a4fc_d9a2_b64f_1190_3fa6_f310),
        (83_280, 0x8f26_cd5c_ad46_561b_87ea_9453_cfd5_2e35),
        (88_676, 0xf19b_6cb6_7aa7_836e_73d5_1320_a3c5_186e),
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
