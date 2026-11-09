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
        (109_371, 0xb4d0_a58f_16bb_82c2_bfcd_6115_674f_bc2b),
        (64_869, 0xdb31_8164_dd59_1240_4592_cc2e_6f1a_adba),
        (67_280, 0xbe65_ad49_3f9f_555b_79f5_3fb0_b1fc_c691),
        (72_861, 0xef7b_25ea_4112_5e79_6594_8004_cab8_8423),
        (78_999, 0xcff9_2154_b06e_a1d6_77d3_7bdc_9d8e_8351),
        (84_580, 0xb146_6c61_1a2f_5acf_74e3_8174_44bd_eeeb),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (113_405, 0x5eff_7820_df4a_a39c_4f5a_a086_3ae8_86e1),
        (65_232, 0x4ac6_dadb_5aea_0f72_32d1_f6e6_e8d9_0a56),
        (67_643, 0x38e7_6a91_8687_a498_e322_e84b_baf3_4d23),
        (73_224, 0x0040_ee51_e5d7_838c_eae7_8dac_46fc_2f39),
        (79_362, 0xeee9_0930_fa98_2329_e289_eeac_bdbf_68cb),
        (84_943, 0x5d51_a5e7_d436_e840_e4b5_d5f5_145d_9f59),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (67_333, 0x9da1_b2c2_4876_348e_b148_c841_ab29_0e05),
        (69_744, 0x10ea_c75d_8e52_da73_f959_a259_24ad_f176),
        (75_325, 0x160f_71c8_c209_c262_f5ec_ce57_3693_233e),
        (81_463, 0x82ab_b023_39ea_9bf8_e4e3_9461_3ae5_947c),
        (87_044, 0x5d2e_b83f_d027_52b5_7044_c112_a250_bb8c),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (68_727, 0xb57a_400c_b555_ba93_6b50_252a_c26e_3e6e),
        (71_138, 0xf009_102b_d6c7_2b8b_7d77_0d7c_d386_99d1),
        (76_719, 0xb47a_d61f_1377_0edb_57aa_ad90_7d55_e4dd),
        (82_857, 0x60bc_230e_90a4_9f79_1a24_8059_56cc_754b),
        (88_438, 0x9fb9_fa55_3792_6133_b0eb_baaa_76a0_5a8f),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (67_690, 0xc3e5_44c2_b5b6_4127_4425_9dd4_97fb_8b76),
        (70_101, 0x1c54_9832_48e2_1300_c27b_09e2_634e_9d13),
        (75_682, 0x004e_4a1c_eb0d_1da9_fc33_fef6_d779_199b),
        (81_820, 0x7f5e_2f44_dfcc_d916_ac7d_ab33_9853_dbf9),
        (87_401, 0xd06a_1522_e127_2712_f838_7a62_eb57_35f1),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (69_084, 0x5313_236e_9848_e9b2_946e_3498_3897_1043),
        (71_495, 0x6522_a18f_eb35_c84f_bfc2_5d1d_dca2_1836),
        (77_076, 0x4982_17df_f7a7_8fb4_ec7c_cf6b_4659_0406),
        (83_214, 0x03d4_6507_fda4_5776_2d4a_abeb_aa3d_4b90),
        (88_795, 0x11f4_445a_5b95_64e7_4ee1_f8fc_3b4e_bca8),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (109_790, 0xd613_0cb8_e05d_19f2_45d1_09f3_8749_f19d),
        (65_288, 0x57fc_2250_8480_7ca8_5747_ed9b_6a3a_34aa),
        (67_699, 0xcc37_c74a_08f7_c1c8_ea40_e499_fa4e_e74f),
        (73_280, 0xde76_5617_84f0_c0bd_48c4_46d4_83d7_90af),
        (79_418, 0x64d4_cfbb_8b22_b48f_0673_1021_9deb_5a11),
        (84_999, 0x6897_2566_83c2_fee3_7917_3260_0215_cad1),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (111_184, 0xa087_2c85_d593_f066_f476_7596_0601_33ee),
        (66_682, 0xa48a_a4f8_1edf_407c_2376_8ebc_2b23_af11),
        (69_093, 0xbbb7_fb31_9a39_5bfc_12b8_b3b5_b8e7_fbf0),
        (74_674, 0xb18e_94af_4be5_e50b_f3fa_7af8_c479_3ec4),
        (80_812, 0xe420_2834_30b2_0972_f20d_7cb2_566f_fd8e),
        (86_393, 0xf4e3_e5ac_109f_6c41_5ea0_d977_d95b_6ac2),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (113_824, 0x2f5d_59db_d9ee_2792_00bc_8f83_9a8d_905f),
        (65_651, 0x8766_b263_a874_1cd0_925e_d15c_c9c4_2bc0),
        (68_062, 0x5b3c_3e40_42d7_cf7d_d4a3_d35c_b470_7633),
        (73_643, 0x3945_a858_fb1f_0baf_1466_cec8_fcc4_7ff7),
        (79_781, 0x115f_cd9d_3b04_8336_4175_3050_5406_39c9),
        (85_362, 0x421f_f5a3_63bc_520e_6d8e_7418_77b1_333d),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (115_218, 0x0da6_b367_dada_3d1c_0144_084a_fda4_200c),
        (67_045, 0x8c8b_8213_3783_5875_457a_3668_29c4_290b),
        (69_456, 0x8d29_5f32_6a5a_90c7_84e4_223f_c4e0_9924),
        (75_037, 0x912b_c809_e573_69d4_313c_7501_c8bb_157c),
        (81_175, 0xf9ee_eb6c_ae9c_8f0b_9fd8_de06_39bb_2e36),
        (86_756, 0x89bf_f680_e186_b5f6_31e5_4ed3_e016_403e),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (67_333, 0x9da1_b2c2_4876_348e_b148_c841_ab29_0e05),
        (69_744, 0x10ea_c75d_8e52_da73_f959_a259_24ad_f176),
        (75_325, 0x160f_71c8_c209_c262_f5ec_ce57_3693_233e),
        (81_463, 0x82ab_b023_39ea_9bf8_e4e3_9461_3ae5_947c),
        (87_044, 0x5d2e_b83f_d027_52b5_7044_c112_a250_bb8c),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (68_727, 0xb57a_400c_b555_ba93_6b50_252a_c26e_3e6e),
        (71_138, 0xf009_102b_d6c7_2b8b_7d77_0d7c_d386_99d1),
        (76_719, 0xb47a_d61f_1377_0edb_57aa_ad90_7d55_e4dd),
        (82_857, 0x60bc_230e_90a4_9f79_1a24_8059_56cc_754b),
        (88_438, 0x9fb9_fa55_3792_6133_b0eb_baaa_76a0_5a8f),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (67_690, 0xc3e5_44c2_b5b6_4127_4425_9dd4_97fb_8b76),
        (70_101, 0x1c54_9832_48e2_1300_c27b_09e2_634e_9d13),
        (75_682, 0x004e_4a1c_eb0d_1da9_fc33_fef6_d779_199b),
        (81_820, 0x7f5e_2f44_dfcc_d916_ac7d_ab33_9853_dbf9),
        (87_401, 0xd06a_1522_e127_2712_f838_7a62_eb57_35f1),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (69_084, 0x5313_236e_9848_e9b2_946e_3498_3897_1043),
        (71_495, 0x6522_a18f_eb35_c84f_bfc2_5d1d_dca2_1836),
        (77_076, 0x4982_17df_f7a7_8fb4_ec7c_cf6b_4659_0406),
        (83_214, 0x03d4_6507_fda4_5776_2d4a_abeb_aa3d_4b90),
        (88_795, 0x11f4_445a_5b95_64e7_4ee1_f8fc_3b4e_bca8),
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
        (112_893, 0x068f_bf35_b849_98d7_276e_3e8e_e360_0cdd),
        (67_333, 0x9da1_b2c2_4876_348e_b148_c841_ab29_0e05),
        (69_744, 0x10ea_c75d_8e52_da73_f959_a259_24ad_f176),
        (75_325, 0x160f_71c8_c209_c262_f5ec_ce57_3693_233e),
        (81_463, 0x82ab_b023_39ea_9bf8_e4e3_9461_3ae5_947c),
        (87_044, 0x5d2e_b83f_d027_52b5_7044_c112_a250_bb8c),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (114_287, 0xa9be_fbd4_619e_49a6_362f_fb13_9ae6_514e),
        (68_727, 0xb57a_400c_b555_ba93_6b50_252a_c26e_3e6e),
        (71_138, 0xf009_102b_d6c7_2b8b_7d77_0d7c_d386_99d1),
        (76_719, 0xb47a_d61f_1377_0edb_57aa_ad90_7d55_e4dd),
        (82_857, 0x60bc_230e_90a4_9f79_1a24_8059_56cc_754b),
        (88_438, 0x9fb9_fa55_3792_6133_b0eb_baaa_76a0_5a8f),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (112_959, 0xd6ae_f592_dd4c_3a55_f464_498d_1a6d_ba68),
        (67_399, 0x2169_7d7e_08b3_c59b_7379_2693_1da6_526c),
        (69_810, 0x3ef8_6b41_01a9_6885_9597_fe4c_4c24_6f57),
        (75_391, 0x7707_d753_ebd2_9928_abf7_c388_e7ee_733f),
        (81_529, 0xfd8e_2f9d_96e9_2cd9_10ee_ee09_b99d_18bd),
        (87_110, 0x0e08_70bf_0cbf_4f4c_a6bc_882b_8d08_8f3d),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (114_353, 0x5f06_5d1f_a862_1bb2_ef37_47cc_a6c3_4b4f),
        (68_793, 0x62d7_fc90_fb05_3431_10d5_25bc_e8ef_54cf),
        (71_204, 0xabcd_3afa_ef1f_d895_8f7c_314c_cc1c_fdf0),
        (76_785, 0x947a_f319_b378_c629_824f_15da_1967_84c4),
        (82_923, 0xeae9_9425_4dd3_cbe2_f23b_5e51_a370_3992),
        (88_504, 0x9a1b_1b1b_486d_5b50_a407_b93a_2bdb_65fe),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (117_241, 0x1496_600e_6649_47e0_dde6_2f3c_7259_806f),
        (67_690, 0xc3e5_44c2_b5b6_4127_4425_9dd4_97fb_8b76),
        (70_101, 0x1c54_9832_48e2_1300_c27b_09e2_634e_9d13),
        (75_682, 0x004e_4a1c_eb0d_1da9_fc33_fef6_d779_199b),
        (81_820, 0x7f5e_2f44_dfcc_d916_ac7d_ab33_9853_dbf9),
        (87_401, 0xd06a_1522_e127_2712_f838_7a62_eb57_35f1),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (118_635, 0x12d6_8d85_39e3_404f_585d_86f6_01d4_1756),
        (69_084, 0x5313_236e_9848_e9b2_946e_3498_3897_1043),
        (71_495, 0x6522_a18f_eb35_c84f_bfc2_5d1d_dca2_1836),
        (77_076, 0x4982_17df_f7a7_8fb4_ec7c_cf6b_4659_0406),
        (83_214, 0x03d4_6507_fda4_5776_2d4a_abeb_aa3d_4b90),
        (88_795, 0x11f4_445a_5b95_64e7_4ee1_f8fc_3b4e_bca8),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (117_307, 0xa6b8_60f3_a03a_0be9_c128_b751_e72e_5b8c),
        (67_756, 0x00dd_86af_fe2f_4aa6_ba6f_32eb_8eb9_6c23),
        (70_167, 0x8c08_e058_ef86_3219_7272_3d59_8c7c_394e),
        (75_748, 0x58c7_c5b9_e437_8db8_2191_66ad_7b97_02e6),
        (81_886, 0x7d7f_6288_a865_90d5_f052_507b_ef77_41dc),
        (87_467, 0xbe76_610c_64ee_34c7_a0ca_1133_20f0_4a04),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (118_701, 0x263a_0b8b_cc5e_bf0d_7e60_79e8_d348_4be9),
        (69_150, 0x0d47_3f55_6098_bbb6_d6fb_432c_5225_3d16),
        (71_561, 0xcd49_e627_3044_796a_4e28_a0eb_c34c_766b),
        (77_142, 0x5cba_eba2_921c_fc1f_d3ed_06b4_13ba_5c5b),
        (83_280, 0x8f26_cd5c_ad46_561b_87ea_9453_cfd5_2e35),
        (88_861, 0x6256_abc6_8a28_f0ee_7b50_bc00_7c23_b70d),
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
