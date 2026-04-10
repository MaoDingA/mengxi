#include <stdint.h>
#include <stdatomic.h>
#include <string.h>

/* ============================================================
 * FFI Constants — must match MoonBit enum definitions
 * ============================================================ */

/* ACES Color Space Tags — must match MoonBit ACESColorSpace enum */
#define ACES_AP0       10  /* ACES 2065-1 (AP0) */
#define ACES_AP1       11  /* ACEScg (AP1) */
#define ACES_ACESCCT   12  /* ACEScct */
#define ACES_REC709    20  /* Rec709 / sRGB */

/*
 * FFI Return Value Semantics:
 *
 *   Function                        Success    Failure Codes
 *   ----------------------------    --------  ------------------
 *   mengxi_compute_fingerprint        > 0       -1,-2,-3
 *   mengxi_generate_lut              > 0       -1,-2,-3
 *   mengxi_aces_transform            > 0       -1,-2,-3,-4
 *   mengxi_srgb_to_oklab             > 0       -1,-2
 *   mengxi_oklab_to_srgb             > 0       -1,-2
 *   mengxi_acescct_to_oklab          > 0       -1,-2
 *   mengxi_oklab_to_acescct          > 0       -1,-2
 *   mengxi_linear_to_oklab           > 0       -1,-2
 *   mengxi_oklab_to_linear           > 0       -1,-2
 *   mengxi_extract_grading_features  >= 0      -1,-3
 *   mengxi_bhattacharyya_distance    >= 0      -1,-3
 *   mengxi_extract_center_column     > 0       -1,-3
 *   mengxi_stitch_fingerprint_strip   > 0       -1,-3
 *   mengxi_cineiris_transform         > 0       -1,-3
 *   mengxi_extract_color_dna          > 0       -1,-3
 *   mengxi_compare_color_dna          > 0       -1,-3
 *   mengxi_detect_scene_boundaries    > 0       -1,-3
 *   mengxi_compute_mood_timeline      > 0       -1,-3
 *   mengxi_generate_color_transfer_lut > 0      -1,-3
 *   mengxi_extract_temporal_features   > 0      -1,-3
 *   mengxi_extract_frame_scatter      > 0      -1,-3
 *   mengxi_compute_scatter_density     > 0      -1,-3
 *   mengxi_detect_dominant_pairs      > 0      -1,-3
 *   mengxi_compute_vectorscope_density > 0      -1,-3
 *   mengxi_classify_color_distribution  > 0     -1,-3
 *   mengxi_display_p3_to_oklab         > 0     -1,-2
 *   mengxi_oklab_to_display_p3         > 0     -1,-2
 *
 * Note: extract_grading_features returns 0 on success (not pixel count)
 * because it writes to multiple output arrays. bhattacharyya_distance
 * returns 0 on success (score written to output array). Both use >= 0
 * to distinguish success (0 = valid result) from failure (< 0).
 */

/* MoonBit Ref-Counting Adjustments
 * MoonBit's FFI automatically calls incref/decref on FixedArray parameters.
 * We bump ref counts to survive these automatic decref calls, preventing
 * premature freeing of our arrays. We handle cleanup manually.
 *
 * Values depend on the number of FixedArray parameters and exit paths:
 * - RC_BUMP_SINGLE: Single array with simple control flow
 * - RC_BUMP_DOUBLE: Two arrays with multiple exit paths
 * - RC_BUMP_MULTI:  Many arrays (6+), generous safety margin
 */

/*
 * RC_BUMP Derivation Notes (empirical, verified via stress testing):
 *
 * MoonBit native backend generates incref/decref calls for FixedArray params:
 *   - 1 incref per FixedArray parameter on function entry
 *   - 1 decref per FixedArray parameter on each exit path (early return + normal)
 *   - Additional decref may occur in loops over FixedArray data
 *
 * Values were determined by:
 *   1. Inspecting _build/native/debug/build/lib/lib.c generated code
 *   2. Stress testing with large arrays (10000+ elements) x 1000 calls
 *   3. Running under AddressSanitizer (ASan) to detect use-after-free
 *
 * If MoonBit compiler changes its FFI codegen pattern, these values MUST be re-verified.
 * Symptoms of incorrect RC_BUMP: intermittent crashes, ASan errors, or
 *   heap corruption under high call frequency.
 */
#define RC_BUMP_SINGLE  2   /* Single array, 1-2 exit paths */
#define RC_BUMP_DOUBLE  6   /* Two arrays, 3-4 exit paths */
#define RC_BUMP_QUAD    8   /* Four or more arrays, generous margin */

/* Grading Features Constants */
#define GRADING_HIST_BINS_DEFAULT 64  /* Default histogram bin count */
#define GRADING_MOMENTS_COUNT 12      /* L/a/b × 4 moments (mean/std/skew/kurt) */

/* C wrapper for MoonBit FFI.
   MoonBit FixedArray[Double] is a managed object (ref-counted), NOT a raw C pointer.
   MoonBit's generated code calls moonbit_decref on all FixedArray parameters
   before returning, which would free our arrays. We bump the ref count to
   prevent this, then copy results and clean up ourselves.
   It is NOT included in native-stub (to avoid test linker issues). */

struct moonbit_object {
  int32_t rc;
  uint32_t meta;
};

#define Moonbit_object_header(obj) ((struct moonbit_object*)(obj) - 1)

extern void moonbit_runtime_init(int argc, char** argv);
extern double* moonbit_make_double_array(int32_t len, double value);
extern void moonbit_drop_object(void*);
extern int32_t _M0FP216mengxi_2dmoonbit3lib28mengxi__compute__fingerprint(
  int32_t,
  double*,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib23mengxi__aces__transform(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib21mengxi__generate__lut(
  int32_t,
  int32_t,
  int32_t,
  double*,
  int32_t
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib23mengxi__srgb__to__oklab(
  int32_t,
  double*,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib23mengxi__oklab__to__srgb(
  int32_t,
  double*,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib26mengxi__acescct__to__oklab(
  int32_t,
  double*,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib26mengxi__oklab__to__acescct(
  int32_t,
  double*,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib25mengxi__linear__to__oklab(
  int32_t,
  double*,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib25mengxi__oklab__to__linear(
  int32_t,
  double*,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib34mengxi__extract__grading__features(
  double*,   /* pixel_ptr */
  int32_t,   /* pixel_len */
  int32_t,   /* color_space_tag */
  int32_t,   /* hist_bins */
  double*,   /* hist_l_ptr */
  double*,   /* hist_a_ptr */
  double*,   /* hist_b_ptr */
  double*,   /* moments_ptr */
  double*,   /* hist_len_ptr */
  double*    /* moments_len_ptr */
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib31mengxi__bhattacharyya__distance(
  double*,
  double*,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib31mengxi__extract__center__column(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib34mengxi__stitch__fingerprint__strip(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib27mengxi__cineiris__transform(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib27mengxi__extract__color__dna(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib27mengxi__compare__color__dna(
  double*,
  double*,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib33mengxi__detect__scene__boundaries(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib31mengxi__compute__mood__timeline(
  int32_t,
  double*,
  int32_t,
  int32_t,
  double*,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib38mengxi__generate__color__transfer__lut(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib35mengxi__extract__temporal__features(
  int32_t,
  double*,
  int32_t,
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  double*
);

extern int32_t _M0FP216mengxi_2dmoonbit3lib31mengxi__extract__frame__scatter(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib33mengxi__compute__scatter__density(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib31mengxi__detect__dominant__pairs(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  double*
);

static atomic_int runtime_initialized = 0;



static void ensure_runtime_init(void) {
  if (!atomic_load(&runtime_initialized)) {
    static char argv0[] = "mengxi";
    static char* argv[] = { argv0, NULL };
    moonbit_runtime_init(1, argv);
    atomic_store(&runtime_initialized, 1);
  }
}

int32_t mengxi_compute_fingerprint(
  int32_t data_len,
  double* data_ptr,
  int32_t color_tag,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  /* Allocate MoonBit-managed arrays */
  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_data);
    return -3;
  }

  /* Copy input data into MoonBit array */
  memcpy(mb_data, data_ptr, data_len * sizeof(double));

  /*
   * MoonBit's generated code calls moonbit_incref/decref on FixedArray params.
   * It also calls moonbit_decref on out_ptr before returning (early paths + normal).
   * We bump ref counts so that MoonBit's decrefs don't trigger moonbit_drop_object,
   * which would free our arrays prematurely. We handle cleanup ourselves.
   */
  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;  /* 3 incref calls + 1 decref in loop exit */
  Moonbit_object_header(mb_out)->rc += RC_BUMP_SINGLE;    /* 1 decref on each early return path + 1 at end */

  /* Call MoonBit function */
  int32_t result = _M0FP216mengxi_2dmoonbit3lib28mengxi__compute__fingerprint(
    data_len, mb_data, color_tag, out_len, mb_out
  );

  /* Copy output data back to Rust buffer before freeing */
  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  /* Free MoonBit arrays */
  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);

  return result;
}

int32_t mengxi_generate_lut(
  int32_t grid_size,
  int32_t src_cs,
  int32_t dst_cs,
  double* out_ptr,
  int32_t out_len
) {
  ensure_runtime_init();

  /* Allocate MoonBit-managed array for output */
  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) return -2;

  /* Bump ref count to survive MoonBit's decref on FixedArray param.
   * mengxi_generate_lut has 3 early return paths + 1 normal exit. */
  Moonbit_object_header(mb_out)->rc += RC_BUMP_QUAD;

  /* Call MoonBit function */
  int32_t result = _M0FP216mengxi_2dmoonbit3lib21mengxi__generate__lut(
    grid_size, src_cs, dst_cs, mb_out, out_len
  );

  /* Copy output data back to Rust buffer before freeing */
  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  /* Free MoonBit array */
  moonbit_drop_object(mb_out);

  return result;
}
int32_t mengxi_aces_transform(
  int32_t data_len,
  double* data_ptr,
  int32_t src_tag,
  int32_t dst_tag,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  /* Allocate MoonBit-managed arrays */
  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_data);
    return -3;
  }

  /* Copy input data into MoonBit array */
  memcpy(mb_data, data_ptr, data_len * sizeof(double));

  /*
   * Bump ref counts to survive MoonBit's incref/decref on FixedArray params.
   * mengxi_aces_transform has 3 early return paths + 1 normal exit, each
   * decrefing both arrays. Generous bump to prevent premature freeing.
   */
  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  /* Call MoonBit function */
  int32_t result = _M0FP216mengxi_2dmoonbit3lib23mengxi__aces__transform(
    data_len, mb_data, src_tag, dst_tag, out_len, mb_out
  );

  /* Copy output data back to Rust buffer before freeing */
  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  /* Free MoonBit arrays */
  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);

  return result;
}

int32_t mengxi_srgb_to_oklab(
  int32_t data_len,
  double* data_ptr,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (data_len <= 0) return -1;

  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_data);
    return -3;
  }

  memcpy(mb_data, data_ptr, data_len * sizeof(double));

  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib23mengxi__srgb__to__oklab(
    data_len, mb_data, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);

  return result;
}

int32_t mengxi_oklab_to_srgb(
  int32_t data_len,
  double* data_ptr,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (data_len <= 0) return -1;

  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_data);
    return -3;
  }

  memcpy(mb_data, data_ptr, data_len * sizeof(double));

  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib23mengxi__oklab__to__srgb(
    data_len, mb_data, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);

  return result;
}

int32_t mengxi_acescct_to_oklab(
  int32_t data_len,
  double* data_ptr,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (data_len <= 0) return -1;

  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_data);
    return -3;
  }

  memcpy(mb_data, data_ptr, data_len * sizeof(double));

  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib26mengxi__acescct__to__oklab(
    data_len, mb_data, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);

  return result;
}

int32_t mengxi_oklab_to_acescct(
  int32_t data_len,
  double* data_ptr,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (data_len <= 0) return -1;

  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_data);
    return -3;
  }

  memcpy(mb_data, data_ptr, data_len * sizeof(double));

  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib26mengxi__oklab__to__acescct(
    data_len, mb_data, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);

  return result;
}

int32_t mengxi_linear_to_oklab(
  int32_t data_len,
  double* data_ptr,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (data_len <= 0) return -1;

  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_data);
    return -3;
  }

  memcpy(mb_data, data_ptr, data_len * sizeof(double));

  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib25mengxi__linear__to__oklab(
    data_len, mb_data, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);

  return result;
}

int32_t mengxi_oklab_to_linear(
  int32_t data_len,
  double* data_ptr,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (data_len <= 0) return -1;

  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_data);
    return -3;
  }

  memcpy(mb_data, data_ptr, data_len * sizeof(double));

  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib25mengxi__oklab__to__linear(
    data_len, mb_data, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);

  return result;
}

/* Default histogram bin count (used when caller passes 0) */
#define GRADING_HIST_BINS_DEFAULT 64

int32_t mengxi_extract_grading_features(
  int32_t pixel_len,
  double* pixel_ptr,
  int32_t color_space_tag,
  int32_t hist_bins,
  double* hist_l_ptr,
  double* hist_a_ptr,
  double* hist_b_ptr,
  double* moments_ptr,
  double* out_hist_len,
  double* out_moments_len
) {
  ensure_runtime_init();

  if (pixel_len <= 0) return -1;

  /* Use default if caller passes 0 or negative */
  int32_t bins = hist_bins > 0 ? hist_bins : GRADING_HIST_BINS_DEFAULT;

  /* Allocate MoonBit-managed arrays */
  double* mb_pixels = moonbit_make_double_array(pixel_len, 0.0);
  if (!mb_pixels) return -3;

  double* mb_hist_l = moonbit_make_double_array(bins, 0.0);
  double* mb_hist_a = moonbit_make_double_array(bins, 0.0);
  double* mb_hist_b = moonbit_make_double_array(bins, 0.0);
  double* mb_moments = moonbit_make_double_array(GRADING_MOMENTS_COUNT, 0.0);
  double* mb_hist_len = moonbit_make_double_array(1, 0.0);
  double* mb_moments_len = moonbit_make_double_array(1, 0.0);
  if (!mb_hist_l || !mb_hist_a || !mb_hist_b || !mb_moments || !mb_hist_len || !mb_moments_len) {
    if (mb_moments_len) moonbit_drop_object(mb_moments_len);
    if (mb_hist_len) moonbit_drop_object(mb_hist_len);
    if (mb_moments) moonbit_drop_object(mb_moments);
    if (mb_hist_b) moonbit_drop_object(mb_hist_b);
    if (mb_hist_a) moonbit_drop_object(mb_hist_a);
    if (mb_hist_l) moonbit_drop_object(mb_hist_l);
    moonbit_drop_object(mb_pixels);
    return -3;
  }

  /* Copy input pixel data into MoonBit array */
  memcpy(mb_pixels, pixel_ptr, pixel_len * sizeof(double));

  /* Bump ref counts to survive MoonBit's incref/decref on FixedArray params.
   * 9 FixedArray params × generous bump. */
  Moonbit_object_header(mb_pixels)->rc += RC_BUMP_QUAD;
  Moonbit_object_header(mb_hist_l)->rc += RC_BUMP_QUAD;
  Moonbit_object_header(mb_hist_a)->rc += RC_BUMP_QUAD;
  Moonbit_object_header(mb_hist_b)->rc += RC_BUMP_QUAD;
  Moonbit_object_header(mb_moments)->rc += RC_BUMP_QUAD;
  Moonbit_object_header(mb_hist_len)->rc += RC_BUMP_QUAD;
  Moonbit_object_header(mb_moments_len)->rc += RC_BUMP_QUAD;

  /* Call MoonBit compound function */
  int32_t result = _M0FP216mengxi_2dmoonbit3lib34mengxi__extract__grading__features(
    mb_pixels, pixel_len, color_space_tag, bins,
    mb_hist_l, mb_hist_a, mb_hist_b,
    mb_moments, mb_hist_len, mb_moments_len
  );

  /* Returns 0 on success (not pixel count); use >= 0 for success check */
  if (result >= 0) {
    memcpy(hist_l_ptr, mb_hist_l, bins * sizeof(double));
    memcpy(hist_a_ptr, mb_hist_a, bins * sizeof(double));
    memcpy(hist_b_ptr, mb_hist_b, bins * sizeof(double));
    memcpy(moments_ptr, mb_moments, GRADING_MOMENTS_COUNT * sizeof(double));
    memcpy(out_hist_len, mb_hist_len, 1 * sizeof(double));
    memcpy(out_moments_len, mb_moments_len, 1 * sizeof(double));
  }

  /* Free MoonBit arrays (reverse order of creation) */
  moonbit_drop_object(mb_moments_len);
  moonbit_drop_object(mb_hist_len);
  moonbit_drop_object(mb_moments);
  moonbit_drop_object(mb_hist_b);
  moonbit_drop_object(mb_hist_a);
  moonbit_drop_object(mb_hist_l);
  moonbit_drop_object(mb_pixels);

  return result;
}

/* ============================================================
 * mengxi_bhattacharyya_distance — Bhattacharyya similarity
 *
 * Story 3.1: Bhattacharyya Distance Candidate Ranking
 * Pattern follows mengxi_extract_grading_features (lines 464-541)
 * ============================================================ */

int32_t mengxi_bhattacharyya_distance(
  double* query_hist,
  double* candidate_hist,
  int32_t hist_len,
  int32_t channels,
  double* out_score
) {
  ensure_runtime_init();

  if (hist_len <= 0 || channels <= 0) return -1;

  int32_t total_len = hist_len * channels;

  /* Allocate MoonBit-managed arrays */
  double* mb_query = moonbit_make_double_array(total_len, 0.0);
  if (!mb_query) return -3;

  double* mb_candidate = moonbit_make_double_array(total_len, 0.0);
  if (!mb_candidate) {
    moonbit_drop_object(mb_query);
    return -3;
  }

  double* mb_out = moonbit_make_double_array(1, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_candidate);
    moonbit_drop_object(mb_query);
    return -3;
  }

  /* Copy input data */
  memcpy(mb_query, query_hist, total_len * sizeof(double));
  memcpy(mb_candidate, candidate_hist, total_len * sizeof(double));

  /* Bump ref counts to survive MoonBit's incref/decref */
  Moonbit_object_header(mb_query)->rc += RC_BUMP_QUAD;
  Moonbit_object_header(mb_candidate)->rc += RC_BUMP_QUAD;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_QUAD;

  /* Call MoonBit function */
  int32_t result = _M0FP216mengxi_2dmoonbit3lib31mengxi__bhattacharyya__distance(
    mb_query, mb_candidate, hist_len, channels, mb_out
  );

  /* Returns 0 on success (score in out_score array); use >= 0 for success check */
  if (result >= 0) {
    memcpy(out_score, mb_out, 1 * sizeof(double));
  }

  /* Free MoonBit arrays (reverse order) */
  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_candidate);
  moonbit_drop_object(mb_query);

  return result;
}

/* ============================================================
 * mengxi_extract_center_column — Movie fingerprint center column
 *
 * Extracts the center column of pixels from a frame.
 * Input: pixels (width * height * 3 doubles)
 * Output: height * 3 doubles (center column RGB)
 * ============================================================ */

int32_t mengxi_extract_center_column(
  int32_t pixel_len,
  double* pixel_ptr,
  int32_t width,
  int32_t height,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (pixel_len <= 0 || width <= 0 || height <= 0) return -1;

  double* mb_pixels = moonbit_make_double_array(pixel_len, 0.0);
  if (!mb_pixels) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_pixels);
    return -3;
  }

  memcpy(mb_pixels, pixel_ptr, pixel_len * sizeof(double));

  Moonbit_object_header(mb_pixels)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib31mengxi__extract__center__column(
    pixel_len, mb_pixels, width, height, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_pixels);

  return result;
}

/* ============================================================
 * mengxi_stitch_fingerprint_strip — Stitch center columns into strip
 *
 * Stitches per-frame center columns into a fingerprint strip.
 * Input: columns (num_frames * frame_height * 3 doubles)
 * Output: num_frames * frame_height * 3 doubles
 * ============================================================ */

int32_t mengxi_stitch_fingerprint_strip(
  int32_t columns_len,
  double* columns_ptr,
  int32_t num_frames,
  int32_t frame_height,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (columns_len <= 0 || num_frames <= 0 || frame_height <= 0) return -1;

  double* mb_columns = moonbit_make_double_array(columns_len, 0.0);
  if (!mb_columns) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_columns);
    return -3;
  }

  memcpy(mb_columns, columns_ptr, columns_len * sizeof(double));

  Moonbit_object_header(mb_columns)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib34mengxi__stitch__fingerprint__strip(
    columns_len, mb_columns, num_frames, frame_height, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_columns);

  return result;
}

/* ============================================================
 * mengxi_cineiris_transform — Cineiris circular iris transform
 *
 * Applies circular iris transformation to fingerprint strip.
 * Input: strip_pixels (strip_width * strip_height * 3 doubles)
 * Output: iris_diameter * iris_diameter * 3 doubles
 * ============================================================ */

int32_t mengxi_cineiris_transform(
  int32_t strip_pixel_len,
  double* strip_pixel_ptr,
  int32_t strip_width,
  int32_t strip_height,
  int32_t iris_diameter,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_pixel_len <= 0 || strip_width <= 0 || strip_height <= 0 || iris_diameter <= 0) return -1;

  double* mb_pixels = moonbit_make_double_array(strip_pixel_len, 0.0);
  if (!mb_pixels) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_pixels);
    return -3;
  }

  memcpy(mb_pixels, strip_pixel_ptr, strip_pixel_len * sizeof(double));

  Moonbit_object_header(mb_pixels)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib27mengxi__cineiris__transform(
    strip_pixel_len, mb_pixels, strip_width, strip_height, iris_diameter, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_pixels);

  return result;
}

/* ============================================================
 * mengxi_extract_color_dna — Extract color DNA from fingerprint strip
 * Output: 18 f64 (avg_L,a,b + hue_distribution[12] + contrast + warmth + saturation)
 * ============================================================ */

int32_t mengxi_extract_color_dna(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib27mengxi__extract__color__dna(
    strip_len, mb_strip, width, height, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_compare_color_dna — Compare two color DNA signatures
 * Output: 4 f64 (overall_similarity, hue_similarity, contrast_diff, warmth_diff)
 * ============================================================ */

int32_t mengxi_compare_color_dna(
  double* dna_a_ptr,
  double* dna_b_ptr,
  int32_t dna_len,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (dna_len <= 0) return -1;

  double* mb_a = moonbit_make_double_array(dna_len, 0.0);
  if (!mb_a) return -3;

  double* mb_b = moonbit_make_double_array(dna_len, 0.0);
  if (!mb_b) {
    moonbit_drop_object(mb_a);
    return -3;
  }

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_b);
    moonbit_drop_object(mb_a);
    return -3;
  }

  memcpy(mb_a, dna_a_ptr, dna_len * sizeof(double));
  memcpy(mb_b, dna_b_ptr, dna_len * sizeof(double));

  Moonbit_object_header(mb_a)->rc += 6;
  Moonbit_object_header(mb_b)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib27mengxi__compare__color__dna(
    mb_a, mb_b, dna_len, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_b);
  moonbit_drop_object(mb_a);

  return result;
}

/* ============================================================
 * mengxi_detect_scene_boundaries — Detect scene changes in fingerprint strip
 * Output: 1 + N*8 f64
 * ============================================================ */

int32_t mengxi_detect_scene_boundaries(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  int32_t threshold_permille,
  int32_t min_scene_frames,
  int32_t max_boundaries,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib33mengxi__detect__scene__boundaries(
    strip_len, mb_strip, width, height,
    threshold_permille, min_scene_frames, max_boundaries,
    out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_compute_mood_timeline — Compute mood timeline from strip + boundaries
 * Output: 1 + segments*6 f64
 * ============================================================ */

int32_t mengxi_compute_mood_timeline(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  double* boundaries_ptr,
  int32_t boundaries_len,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  /* Allocate at least 1 element for avoid 0-length array issues */
  int32_t safe_bounds_len = boundaries_len > 0 ? boundaries_len : 1;
  double* mb_bounds = moonbit_make_double_array(safe_bounds_len, 0.0);
  if (!mb_bounds) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_bounds);
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));
  if (boundaries_len > 0) {
    memcpy(mb_bounds, boundaries_ptr, boundaries_len * sizeof(double));
  }

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_bounds)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib31mengxi__compute__mood__timeline(
    strip_len, mb_strip, width, height,
    mb_bounds, boundaries_len,
    out_len, mb_out  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_bounds);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_generate_color_transfer_lut — Generate color transfer 3D LUT
 * Output: grid_size^3 * 3 f64
 * ============================================================ */

int32_t mengxi_generate_color_transfer_lut(
  int32_t src_len,
  double* src_ptr,
  int32_t src_w,
  int32_t src_h,
  int32_t tgt_len,
  double* tgt_ptr,
  int32_t tgt_w,
  int32_t tgt_h,
  int32_t grid_size,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (src_len <= 0 || src_w <= 0 || src_h <= 0) return -1;
  if (tgt_len <= 0 || tgt_w <= 0 || tgt_h <= 0) return -1;

  double* mb_src = moonbit_make_double_array(src_len, 0.0);
  if (!mb_src) return -3;

  double* mb_tgt = moonbit_make_double_array(tgt_len, 0.0);
  if (!mb_tgt) {
    moonbit_drop_object(mb_src);
    return -3;
  }

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_tgt);
    moonbit_drop_object(mb_src);
    return -3;
  }

  memcpy(mb_src, src_ptr, src_len * sizeof(double));
  memcpy(mb_tgt, tgt_ptr, tgt_len * sizeof(double));

  Moonbit_object_header(mb_src)->rc += 6;
  Moonbit_object_header(mb_tgt)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib38mengxi__generate__color__transfer__lut(
    src_len, mb_src, src_w, src_h,
    tgt_len, mb_tgt, tgt_w, tgt_h,
    grid_size, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_tgt);
  moonbit_drop_object(mb_src);

  return result;
}

/* ============================================================
 * mengxi_extract_temporal_features — Extract temporal features per segment
 * Output: segments * (hist_bins*3 + 12) f64
 * ============================================================ */

int32_t mengxi_extract_temporal_features(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  double* segments_ptr,
  int32_t segments_len,
  int32_t hist_bins,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  double* mb_segs = moonbit_make_double_array(segments_len, 0.0);
  if (!mb_segs) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_segs);
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));
  memcpy(mb_segs, segments_ptr, segments_len * sizeof(double));

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_segs)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib35mengxi__extract__temporal__features(
    strip_len, mb_strip, width, height,
    mb_segs, segments_len, hist_bins,
    out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_segs);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_extract_frame_scatter — Frame-level Oklab scatter on a-b plane
 *
 * For each frame column, samples the center row pixel and converts to Oklab.
 * Output: width * 3 f64 (L, a, b per frame)
 * ============================================================ */

int32_t mengxi_extract_frame_scatter(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib31mengxi__extract__frame__scatter(
    strip_len, mb_strip, width, height, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_compute_scatter_density — Compute a-b scatter density grid
 *
 * Output: grid_size * grid_size f64 (density values normalized to [0,1])
 * ============================================================ */

int32_t mengxi_compute_scatter_density(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  int32_t grid_size,
  int32_t a_range_permille,
  int32_t b_range_permille,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0 || grid_size <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib33mengxi__compute__scatter__density(
    strip_len, mb_strip, width, height,
    grid_size, a_range_permille, b_range_permille,
    out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_detect_dominant_pairs — Detect complementary color pairs
 *
 * Output: 1 + N*4 f64 (count + up to 6 pairs × 4 values)
 * ============================================================ */

int32_t mengxi_detect_dominant_pairs(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  int32_t min_chroma_permille,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib31mengxi__detect__dominant__pairs(
    strip_len, mb_strip, width, height,
    min_chroma_permille, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_compute_vectorscope_density — Compute vectorscope polar density grid
 *
 * Input: strip data (row-major interleaved sRGB [0,1])
 * Output: angle_bins * radius_bins f64 (density, normalized to [0,1])
 * ============================================================ */

extern int32_t _M0FP216mengxi_2dmoonbit3lib37mengxi__compute__vectorscope__density(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  double*
);

int32_t mengxi_compute_vectorscope_density(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  int32_t angle_bins,
  int32_t radius_bins,
  int32_t max_chroma_permille,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0 || angle_bins <= 0 || radius_bins <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib37mengxi__compute__vectorscope__density(
    strip_len, mb_strip, width, height,
    angle_bins, radius_bins, max_chroma_permille,
    out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_classify_color_distribution — Classify strip pixels into 7 color categories
 * ============================================================ */

extern int32_t _M0FP216mengxi_2dmoonbit3lib37mengxi__classify__color__distribution(
  int32_t,
  double*,
  int32_t,
  int32_t,
  int32_t,
  int32_t,
  double*
);

/* Discovered mangled names from build/lib/lib.c */
extern int32_t _M0FP216mengxi_2dmoonbit3lib30mengxi__display__p3__to__oklab(
  int32_t, double*, int32_t, double*
);
extern int32_t _M0FP216mengxi_2dmoonbit3lib30mengxi__oklab__to__display__p3(
  int32_t, double*, int32_t, double*
);

int32_t mengxi_classify_color_distribution(
  int32_t strip_len,
  double* strip_ptr,
  int32_t width,
  int32_t height,
  int32_t min_chroma_permille,
  int32_t out_len,
  double* out_ptr
) {
  ensure_runtime_init();

  if (strip_len <= 0 || width <= 0 || height <= 0) return -1;

  double* mb_strip = moonbit_make_double_array(strip_len, 0.0);
  if (!mb_strip) return -3;

  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) {
    moonbit_drop_object(mb_strip);
    return -3;
  }

  memcpy(mb_strip, strip_ptr, strip_len * sizeof(double));

  Moonbit_object_header(mb_strip)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib37mengxi__classify__color__distribution(
    strip_len, mb_strip, width, height,
    min_chroma_permille,
    out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }

  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_strip);

  return result;
}

/* ============================================================
 * mengxi_display_p3_to_oklab — Display P3 to Oklab conversion
 * Mangled name: lib30 (discovered after build via nm)
 * ============================================================ */
int32_t mengxi_display_p3_to_oklab(
  int32_t data_len, double* data_ptr,
  int32_t out_len, double* out_ptr
) {
  ensure_runtime_init();
  if (data_len <= 0) return -1;

  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;
  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) { moonbit_drop_object(mb_data); return -3; }

  memcpy(mb_data, data_ptr, data_len * sizeof(double));
  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib30mengxi__display__p3__to__oklab(
    data_len, mb_data, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }
  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);
  return result;
}

/* ============================================================
 * mengxi_oklab_to_display_p3 — Oklab to Display P3 conversion
 * Mangled name: lib30 (discovered after build via nm)
 * ============================================================ */
int32_t mengxi_oklab_to_display_p3(
  int32_t data_len, double* data_ptr,
  int32_t out_len, double* out_ptr
) {
  ensure_runtime_init();
  if (data_len <= 0) return -1;

  double* mb_data = moonbit_make_double_array(data_len, 0.0);
  if (!mb_data) return -3;
  double* mb_out = moonbit_make_double_array(out_len, 0.0);
  if (!mb_out) { moonbit_drop_object(mb_data); return -3; }

  memcpy(mb_data, data_ptr, data_len * sizeof(double));
  Moonbit_object_header(mb_data)->rc += RC_BUMP_DOUBLE;
  Moonbit_object_header(mb_out)->rc += RC_BUMP_DOUBLE;

  int32_t result = _M0FP216mengxi_2dmoonbit3lib30mengxi__oklab__to__display__p3(
    data_len, mb_data, out_len, mb_out
  );

  if (result > 0) {
    memcpy(out_ptr, mb_out, out_len * sizeof(double));
  }
  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_data);
  return result;
}
