//! Complete forward source assemblies, including their final screen-space and terrain rewrites.

// Expected byte lengths and FNV-1a/128 were printed by a scratch CPU test in a detached
// worktree of 08ea8958f3bfd353ed99a458c7f7434bb21c0df3, using the production functions
// called below. Only receiver_sources visibility and scratch test wiring changed there.
// Each row is object, terrain, then conventional/cached/layered/cached-layered terrain.
const FINAL_FORWARD_SOURCES: &[[(usize, u128); 6]] = &[
    // ordinary velocity=false ssao=false receiver=Nearest
    [
        (83_470, 0x94b1_a540_a58e_aa8b_fedb_b952_72ff_ac67),
        (47_835, 0xb91f_ae0f_5f60_4dd2_1301_23f0_7eff_8e8b),
        (50_246, 0xa68b_8a9c_88ab_655f_7d1f_2c9c_1ba9_0e6c),
        (55_642, 0x8c07_8d5d_768e_83b4_06a8_684b_6ad5_4e19),
        (61_965, 0x7570_7878_32f1_bd38_9ba9_ccc9_1da8_877e),
        (67_361, 0xc9dc_02a9_2efc_d600_dee9_ba5a_432e_a7bf),
    ],
    // ordinary velocity=false ssao=true receiver=Nearest
    [
        (83_889, 0x2b44_54fb_e542_a41c_f2e5_57dd_875e_47f3),
        (48_254, 0xcc97_cc2d_d99e_6ea0_4495_3bb1_38d3_8af1),
        (50_665, 0xe277_faaf_2d82_7d3e_7b6f_80a8_5e3d_1abc),
        (56_061, 0x3e62_48e4_f729_9bca_ad75_e217_5cfb_ae3f),
        (62_384, 0xc0d9_8c8b_bbb5_f97b_9f02_e83a_def2_056c),
        (67_780, 0x4bfe_6bc3_03c0_de82_8a63_42c3_3587_a16f),
    ],
    // ordinary velocity=false ssao=true receiver=DepthAwareGather
    [
        (85_283, 0x09e7_fb4b_de6c_7ed1_58c4_1af7_4758_8208),
        (49_648, 0x62c9_d264_bf0d_e10f_8a45_cc9f_d957_d6b2),
        (52_059, 0xb20e_1f6e_d64b_f684_45f5_d1e2_99d3_f703),
        (57_455, 0x8b71_d743_5c07_2007_367c_c042_dcfe_972e),
        (63_778, 0xdb0f_bc80_acf9_da06_bdf4_cb84_aafa_d95b),
        (69_174, 0xd5b4_970e_e7b2_b782_01ce_c12a_f354_844e),
    ],
    // ordinary velocity=true ssao=false receiver=Nearest
    [
        (87_504, 0x98a2_1907_8f23_45b1_bb34_3ad5_e324_75fd),
        (48_198, 0xc960_3a60_77cc_6856_e61d_9a4b_9433_b389),
        (50_609, 0xa3a5_d1a3_3b59_8a7e_2725_c716_924a_6d7c),
        (56_005, 0x2d09_c5fc_014c_34cf_264b_de13_14d3_fcc5),
        (62_328, 0x29b7_9133_53ef_f029_8626_6d59_d03c_0a7a),
        (67_724, 0x1a22_9031_2478_e68d_201f_6fc0_5676_b42f),
    ],
    // ordinary velocity=true ssao=true receiver=Nearest
    [
        (87_923, 0x8b17_bf03_d3cc_16ce_2fdc_829d_eed2_6b09),
        (48_617, 0xa039_d968_7e17_7faa_1070_e169_3ffd_b661),
        (51_028, 0xa1e3_36d2_f3e1_4cd0_4475_6746_0d08_88fe),
        (56_424, 0xa9c6_77a8_d5c0_c817_037c_8502_14a9_8f21),
        (62_747, 0x59ca_825c_9901_ea03_9e7f_3801_2245_e08e),
        (68_143, 0xe4b7_6054_0822_1ba4_30b2_f380_00e9_1071),
    ],
    // ordinary velocity=true ssao=true receiver=DepthAwareGather
    [
        (89_317, 0x7f74_babb_d069_4edd_3d64_a9dc_2f0c_7f52),
        (50_011, 0x4ad4_c0b6_5620_3e9e_5aac_8d79_12e8_a23a),
        (52_422, 0x4980_b531_3917_a033_b62d_fcb4_8e86_c439),
        (57_818, 0x3fe2_a929_8e7d_5f93_cf53_33a4_614a_e054),
        (64_141, 0xf778_8cad_b112_89d6_2443_1358_c96f_6139),
        (69_537, 0xb741_fc05_b60a_9483_20df_b387_5a50_8fdc),
    ],
    // receiver ssao=false reflection=false velocity=false receiver=Nearest
    [
        (93_881, 0x79fc_4860_e843_5213_3522_4274_8210_44df),
        (58_246, 0xd34c_303d_39f9_50e8_fafd_d96a_2e93_1f93),
        (60_657, 0xdde9_f053_33c2_9004_763c_7e2f_a447_03e6),
        (66_053, 0x3f49_f407_ff1e_0bfa_56ee_e39a_8b0c_8a3f),
        (72_376, 0x068d_945b_de1c_9adf_1698_d9fc_c85d_6e00),
        (77_772, 0x71ac_41eb_4131_4d4a_2d34_86e0_8605_3a35),
    ],
    // receiver ssao=false reflection=false velocity=true receiver=Nearest
    [
        (97_915, 0x0092_3f3d_1b1a_41bf_920e_18fb_0e18_7e25),
        (58_609, 0x8eb6_f2b2_ed1b_3a74_5512_4a53_e4c1_25f1),
        (61_020, 0xf746_d14f_c6b5_c820_e317_63ae_127d_ae0a),
        (66_416, 0x69fa_7b77_9499_4388_5edb_2165_d991_1d27),
        (72_739, 0x6c2c_d468_5dc2_deaf_717b_3eb7_6a9e_2b90),
        (78_135, 0xc4b7_ed97_0175_eb62_0ecd_f0ce_6619_8441),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=Nearest
    [
        (97_403, 0xf1b2_036c_522d_db15_1fdd_d986_72b6_fddf),
        (60_710, 0x0732_0a77_ba4f_d620_c6dd_c223_8f80_3c4a),
        (63_121, 0x9352_e14a_4368_a514_7972_eb50_210f_5497),
        (68_517, 0x7684_9a9e_053c_bb97_2b1b_96dd_cb82_f64a),
        (74_840, 0x0863_8bab_b3d8_8f58_908b_e984_5f3d_0067),
        (80_236, 0x7850_757a_8a13_c6ef_39e6_580e_3e2e_0cf2),
    ],
    // receiver ssao=false reflection=true velocity=false receiver=DepthAwareGather
    [
        (98_797, 0x4a75_6c08_9062_dba9_21b6_dab2_f077_03ac),
        (62_104, 0xab7d_731f_0f30_3a1b_e53f_11ca_8cc4_a665),
        (64_515, 0x8015_6125_8d4d_11a5_d4a0_ed3b_49fa_d884),
        (69_911, 0x1f6a_98ab_1953_ef91_6e81_a277_8b84_bb57),
        (76_234, 0x79dd_0c5d_4e95_19a2_cf08_70d9_7a5e_b92c),
        (81_630, 0x8471_75f7_7442_d92e_3895_1c99_8f73_3c2f),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=Nearest
    [
        (101_751, 0x21ad_095d_c09c_a2a7_b021_550d_0b96_4d3d),
        (61_067, 0x9d9a_44c0_bf7c_a971_f5ab_3d9b_11ec_1f67),
        (63_478, 0x742f_a6df_f98c_1972_89f7_ca20_88db_fce8),
        (68_874, 0xf67b_ace5_470a_0826_d42c_53ea_d4fc_b74f),
        (75_197, 0xc1bb_945a_c199_1ab0_fa0f_800e_ef0a_9d20),
        (80_593, 0x72bb_fa72_4625_7471_c707_5cab_f495_f4b7),
    ],
    // receiver ssao=false reflection=true velocity=true receiver=DepthAwareGather
    [
        (103_145, 0xd272_f857_9416_b762_edec_b165_eef6_5408),
        (62_461, 0xe496_dcef_17e0_4089_d0c4_9e43_68fa_ec1a),
        (64_872, 0x2127_9159_a1d5_0ee4_d14f_930b_678b_9f01),
        (70_268, 0xf39b_32fe_f78c_d37d_f457_c9d1_cc6b_7148),
        (76_591, 0xddc8_e4e8_d36a_e529_716a_2147_f321_7861),
        (81_987, 0x562d_3d61_fae9_3aab_8568_7a6a_8299_4950),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=Nearest
    [
        (94_300, 0xe8bf_84ac_4e46_c9ac_ee12_803d_f605_fc85),
        (58_665, 0x2b65_3535_c9a0_e605_6ccf_6ef7_8b75_a3db),
        (61_076, 0x6f82_2d1b_01db_1af1_c622_3016_8b77_eb38),
        (66_472, 0x1897_0eb4_7c79_d9eb_5b88_721a_edc0_961b),
        (72_795, 0xb49c_f89f_6389_ea88_4b63_65e5_83db_1718),
        (78_191, 0x1ffc_ea40_8789_7176_0dc3_6a91_c67c_83ab),
    ],
    // receiver ssao=true reflection=false velocity=false receiver=DepthAwareGather
    [
        (95_694, 0x2910_2b6b_bace_b881_b6bb_9d9b_409a_c3f2),
        (60_059, 0x49d8_cecd_b77c_3cbb_65e6_4fd1_8c1b_2fec),
        (62_470, 0x6646_ea9a_81de_ce6e_5423_683b_7463_6613),
        (67_866, 0xbbdb_0401_3f6d_282b_b890_2d79_277a_b43e),
        (74_189, 0xd4b3_f45a_466d_85e4_dd24_250f_b1a8_d30b),
        (79_585, 0x676c_82f8_b43d_7b73_735b_38af_b3a2_023e),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=Nearest
    [
        (98_334, 0xfda0_910d_0d6f_5993_e795_2990_1511_83f3),
        (59_028, 0x24b3_b432_0535_fc05_bacc_6ed4_ca94_f097),
        (61_439, 0x81c0_e913_e36a_161e_8237_4863_b984_db32),
        (66_835, 0x2ba6_0960_fa6a_86e5_695a_3083_9c89_44b5),
        (73_158, 0x7249_1bf3_a3f5_a9f6_b004_a906_df78_4662),
        (78_554, 0x38f3_1157_5709_c797_e068_68e7_b883_fb75),
    ],
    // receiver ssao=true reflection=false velocity=true receiver=DepthAwareGather
    [
        (99_728, 0x5411_097c_8748_9c2a_c767_c93c_6434_b034),
        (60_422, 0xd12b_df8c_c210_25a7_8e28_03c6_7fe1_f5f8),
        (62_833, 0xe8ac_82e5_8bcb_889a_bd35_2333_1bdd_b489),
        (68_229, 0x7517_1956_44ec_e5b5_2498_76a0_e3a2_8824),
        (74_552, 0x40e3_1b1e_9a28_afac_18cc_c177_b261_5929),
        (79_948, 0x313a_a9c4_497b_21ef_4e3d_b3a1_b943_150c),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=Nearest
    [
        (97_403, 0xf1b2_036c_522d_db15_1fdd_d986_72b6_fddf),
        (60_710, 0x0732_0a77_ba4f_d620_c6dd_c223_8f80_3c4a),
        (63_121, 0x9352_e14a_4368_a514_7972_eb50_210f_5497),
        (68_517, 0x7684_9a9e_053c_bb97_2b1b_96dd_cb82_f64a),
        (74_840, 0x0863_8bab_b3d8_8f58_908b_e984_5f3d_0067),
        (80_236, 0x7850_757a_8a13_c6ef_39e6_580e_3e2e_0cf2),
    ],
    // receiver ssao=true reflection=true velocity=false receiver=DepthAwareGather
    [
        (98_797, 0x4a75_6c08_9062_dba9_21b6_dab2_f077_03ac),
        (62_104, 0xab7d_731f_0f30_3a1b_e53f_11ca_8cc4_a665),
        (64_515, 0x8015_6125_8d4d_11a5_d4a0_ed3b_49fa_d884),
        (69_911, 0x1f6a_98ab_1953_ef91_6e81_a277_8b84_bb57),
        (76_234, 0x79dd_0c5d_4e95_19a2_cf08_70d9_7a5e_b92c),
        (81_630, 0x8471_75f7_7442_d92e_3895_1c99_8f73_3c2f),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=Nearest
    [
        (101_751, 0x21ad_095d_c09c_a2a7_b021_550d_0b96_4d3d),
        (61_067, 0x9d9a_44c0_bf7c_a971_f5ab_3d9b_11ec_1f67),
        (63_478, 0x742f_a6df_f98c_1972_89f7_ca20_88db_fce8),
        (68_874, 0xf67b_ace5_470a_0826_d42c_53ea_d4fc_b74f),
        (75_197, 0xc1bb_945a_c199_1ab0_fa0f_800e_ef0a_9d20),
        (80_593, 0x72bb_fa72_4625_7471_c707_5cab_f495_f4b7),
    ],
    // receiver ssao=true reflection=true velocity=true receiver=DepthAwareGather
    [
        (103_145, 0xd272_f857_9416_b762_edec_b165_eef6_5408),
        (62_461, 0xe496_dcef_17e0_4089_d0c4_9e43_68fa_ec1a),
        (64_872, 0x2127_9159_a1d5_0ee4_d14f_930b_678b_9f01),
        (70_268, 0xf39b_32fe_f78c_d37d_f457_c9d1_cc6b_7148),
        (76_591, 0xddc8_e4e8_d36a_e529_716a_2147_f321_7861),
        (81_987, 0x562d_3d61_fae9_3aab_8568_7a6a_8299_4950),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=Nearest
    [
        (86_992, 0x168a_4cb0_9bf2_158f_ea2a_15ef_6be5_64c3),
        (50_299, 0xc7a0_d656_b71c_7060_a910_bd66_9100_ffc8),
        (52_710, 0x2b7c_3c45_ef00_0283_8e13_cb90_1ab6_8d77),
        (58_106, 0x0ac0_c4a7_dbfc_2775_bd63_ecee_f556_2a2a),
        (64_429, 0x1a03_d4c6_f45f_ae29_5a4e_c6f2_e330_3d07),
        (69_825, 0x1578_9ebd_0b3b_c77f_5d88_63ee_edda_9f92),
    ],
    // ssr shadowed=false velocity=false application_ao=false receiver=DepthAwareGather
    [
        (88_386, 0x04c4_3919_eb64_2518_f33b_fb70_ff34_b14c),
        (51_693, 0x224a_5902_3920_8669_2071_acc8_5b15_1e83),
        (54_104, 0x2523_947b_378c_ee75_3208_9524_5560_bb30),
        (59_500, 0x496b_2efa_33f8_06c8_588d_7ea1_ad6a_4e03),
        (65_823, 0xfe82_ccb0_c9bc_97ca_9b53_a37b_2b50_a048),
        (71_219, 0xed98_5511_204a_8eaa_2537_0eb7_2814_d4db),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=Nearest
    [
        (87_058, 0xf3f2_820e_6f2c_c008_a8d5_300f_caf2_0c86),
        (50_365, 0x3497_2dcf_0726_a2b2_2990_46e4_5c3b_0a61),
        (52_776, 0xafe8_de1e_d4c8_9b43_629e_d076_cb7c_abee),
        (58_172, 0x1c45_bd42_e539_8a36_9b72_ec59_08da_9d85),
        (64_495, 0xdaa7_a985_aa50_9b21_93c9_c94b_e40f_4716),
        (69_891, 0x4458_b436_961d_6ec1_9ec5_a65e_7bc2_ec4d),
    ],
    // ssr shadowed=false velocity=false application_ao=true receiver=DepthAwareGather
    [
        (88_452, 0x0669_d03f_37fe_9b4c_5935_078b_e008_2755),
        (51_759, 0x9a5b_0183_51f7_1219_1f93_16d7_1d40_05c2),
        (54_170, 0x682a_d6a2_232d_5b4d_2860_47ae_af05_c1d9),
        (59_566, 0x12ce_bfb5_0e27_29c3_2633_81b9_1102_a900),
        (65_889, 0x9902_dcbb_697b_2bd2_316d_c68c_940a_1ea1),
        (71_285, 0x63ae_0fcc_3f87_f49a_fc58_11d5_68df_4370),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=Nearest
    [
        (91_340, 0x50dd_7894_d0f7_6cab_3a3f_1ce1_d2d3_a209),
        (50_656, 0xeeed_93c6_cc3a_7ffe_a5d2_2c34_0abf_4a2d),
        (53_067, 0xc686_ac5b_7c16_676c_52db_b26d_dadb_1708),
        (58_463, 0x2c46_4bcd_164f_d510_6982_c787_7ab3_d36f),
        (64_786, 0x4ea5_2a96_4294_abe6_4859_845a_6f7e_1f80),
        (70_182, 0x8bce_99c3_60ea_3392_6414_1dd7_b4b9_5a17),
    ],
    // ssr shadowed=false velocity=true application_ao=false receiver=DepthAwareGather
    [
        (92_734, 0x8211_8709_1cca_1b6a_1a2f_3dec_c274_e2a8),
        (52_050, 0x3b18_940e_8849_a5cc_9784_771b_6935_8e70),
        (54_461, 0x4981_036d_2d55_fcb2_56f4_432d_c289_53ed),
        (59_857, 0x9eed_f6c3_7ea2_afcf_fc26_a4f3_b373_cc24),
        (66_180, 0x2a52_a7b0_cdfe_adbe_e3df_25db_2948_2f9d),
        (71_576, 0x54ff_ec19_6ea5_10ae_49c1_6e54_7a56_535c),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=Nearest
    [
        (91_406, 0xe15e_7a26_ac31_9fa4_72c0_08e1_9d91_e25e),
        (50_722, 0xea65_8111_0e6f_230f_346e_31ba_9b73_0498),
        (53_133, 0x877e_37dc_248e_eee2_4324_a183_ec44_c475),
        (58_529, 0x8e9f_2515_6605_43bb_19c0_0921_0540_f778),
        (64_852, 0x1b2a_84e9_4c8a_5230_bea7_68ba_a26f_835d),
        (70_248, 0x5690_7b2d_37a3_af0c_3091_8c4e_2e7a_1908),
    ],
    // ssr shadowed=false velocity=true application_ao=true receiver=DepthAwareGather
    [
        (92_800, 0x956e_c603_6ab4_0bc7_089f_f583_ac94_6a1b),
        (52_116, 0x4869_a06c_81b0_2165_e786_21fe_8a59_9bd5),
        (54_527, 0xcb01_cc45_7135_0668_95cb_aac2_4bee_c360),
        (59_923, 0x54da_bb5e_b5a2_e99d_587c_8d6c_1fdb_80f3),
        (66_246, 0xa40b_a168_ea27_7ea6_7674_ca79_81df_6ea0),
        (71_642, 0x3b23_9611_853c_e694_7b1e_29cb_5da3_a373),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=Nearest
    [
        (97_403, 0xf1b2_036c_522d_db15_1fdd_d986_72b6_fddf),
        (60_710, 0x0732_0a77_ba4f_d620_c6dd_c223_8f80_3c4a),
        (63_121, 0x9352_e14a_4368_a514_7972_eb50_210f_5497),
        (68_517, 0x7684_9a9e_053c_bb97_2b1b_96dd_cb82_f64a),
        (74_840, 0x0863_8bab_b3d8_8f58_908b_e984_5f3d_0067),
        (80_236, 0x7850_757a_8a13_c6ef_39e6_580e_3e2e_0cf2),
    ],
    // ssr shadowed=true velocity=false application_ao=false receiver=DepthAwareGather
    [
        (98_797, 0x4a75_6c08_9062_dba9_21b6_dab2_f077_03ac),
        (62_104, 0xab7d_731f_0f30_3a1b_e53f_11ca_8cc4_a665),
        (64_515, 0x8015_6125_8d4d_11a5_d4a0_ed3b_49fa_d884),
        (69_911, 0x1f6a_98ab_1953_ef91_6e81_a277_8b84_bb57),
        (76_234, 0x79dd_0c5d_4e95_19a2_cf08_70d9_7a5e_b92c),
        (81_630, 0x8471_75f7_7442_d92e_3895_1c99_8f73_3c2f),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=Nearest
    [
        (97_469, 0x5aee_4cdc_a972_b703_af2e_74ff_6798_de0a),
        (60_776, 0x96aa_c5f0_bbc8_06d7_1b04_dd82_a07c_82b3),
        (63_187, 0xd64c_0e10_7a97_db3e_7c7c_d534_8e80_828e),
        (68_583, 0x225c_24c6_145d_415a_fd70_6c70_182b_0825),
        (74_906, 0x41d7_3ec6_4668_df77_1075_f1be_9092_6576),
        (80_302, 0x54b0_8e45_995d_8779_7469_25cb_4455_de2d),
    ],
    // ssr shadowed=true velocity=false application_ao=true receiver=DepthAwareGather
    [
        (98_863, 0x41df_b6ae_8818_1dd4_c2b3_3003_3fea_7635),
        (62_170, 0x1b72_ce98_e6fb_15f0_cf90_508b_b483_63bc),
        (64_581, 0xb402_a037_b220_7395_36cf_e9b6_39ee_56b5),
        (69_977, 0x1c4e_5a35_b48c_4130_067b_6eb4_a1c7_3f5c),
        (76_300, 0x4907_a6cf_5278_dac4_182e_2736_01e3_9d0d),
        (81_696, 0xca3e_dcba_61a6_7d1e_175f_6344_5fb1_c73c),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=Nearest
    [
        (101_751, 0x21ad_095d_c09c_a2a7_b021_550d_0b96_4d3d),
        (61_067, 0x9d9a_44c0_bf7c_a971_f5ab_3d9b_11ec_1f67),
        (63_478, 0x742f_a6df_f98c_1972_89f7_ca20_88db_fce8),
        (68_874, 0xf67b_ace5_470a_0826_d42c_53ea_d4fc_b74f),
        (75_197, 0xc1bb_945a_c199_1ab0_fa0f_800e_ef0a_9d20),
        (80_593, 0x72bb_fa72_4625_7471_c707_5cab_f495_f4b7),
    ],
    // ssr shadowed=true velocity=true application_ao=false receiver=DepthAwareGather
    [
        (103_145, 0xd272_f857_9416_b762_edec_b165_eef6_5408),
        (62_461, 0xe496_dcef_17e0_4089_d0c4_9e43_68fa_ec1a),
        (64_872, 0x2127_9159_a1d5_0ee4_d14f_930b_678b_9f01),
        (70_268, 0xf39b_32fe_f78c_d37d_f457_c9d1_cc6b_7148),
        (76_591, 0xddc8_e4e8_d36a_e529_716a_2147_f321_7861),
        (81_987, 0x562d_3d61_fae9_3aab_8568_7a6a_8299_4950),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=Nearest
    [
        (101_817, 0x5df0_6200_c056_ba25_ef8c_14f6_74f4_993a),
        (61_133, 0xe4de_85ef_cd7d_0fa4_b6ef_00a8_f9c4_be2a),
        (63_544, 0xea5e_5cb9_8d67_d7d8_7b68_f97a_2c63_6255),
        (68_940, 0xdc45_2838_5efc_7321_589f_668f_2e1e_59d8),
        (75_263, 0x4f17_5d69_0bbc_33db_a6e2_af15_1533_047d),
        (80_659, 0x9add_0fbd_7f1e_d693_5de8_3f6f_b7ba_f4a8),
    ],
    // ssr shadowed=true velocity=true application_ao=true receiver=DepthAwareGather
    [
        (103_211, 0x46d1_4264_b672_2e71_a91a_3ab9_12df_567b),
        (62_527, 0xeab0_bfab_a01f_2872_a7e2_a731_67c0_0a17),
        (64_938, 0x7b08_3f6c_b3ad_dfef_699d_7ff2_f0c7_10ac),
        (70_334, 0x4905_c9d3_aba3_d39a_c2a7_10b3_f3fc_c65f),
        (76_657, 0x3435_7150_9d6a_3866_6044_8223_d6ef_ed9c),
        (82_053, 0xf66a_423e_a03e_07a3_df17_f29c_bc00_0e9f),
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
