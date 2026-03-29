#include <stdint.h>
#include <stdatomic.h>
#include <string.h>

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
  Moonbit_object_header(mb_data)->rc += 4;  /* 3 incref calls + 1 decref in loop exit */
  Moonbit_object_header(mb_out)->rc += 2;   /* 1 decref on each early return path + 1 at end */

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
  Moonbit_object_header(mb_out)->rc += 8;

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
  Moonbit_object_header(mb_data)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

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

  Moonbit_object_header(mb_data)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

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

  Moonbit_object_header(mb_data)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

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

  Moonbit_object_header(mb_data)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

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

  Moonbit_object_header(mb_data)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

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

  Moonbit_object_header(mb_data)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

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

  Moonbit_object_header(mb_data)->rc += 6;
  Moonbit_object_header(mb_out)->rc += 6;

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
/* Moments count: mean + stddev for each of L, a, b */
#define GRADING_MOMENTS_COUNT 6

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
  Moonbit_object_header(mb_pixels)->rc += 8;
  Moonbit_object_header(mb_hist_l)->rc += 8;
  Moonbit_object_header(mb_hist_a)->rc += 8;
  Moonbit_object_header(mb_hist_b)->rc += 8;
  Moonbit_object_header(mb_moments)->rc += 8;
  Moonbit_object_header(mb_hist_len)->rc += 8;
  Moonbit_object_header(mb_moments_len)->rc += 8;

  /* Call MoonBit compound function */
  int32_t result = _M0FP216mengxi_2dmoonbit3lib34mengxi__extract__grading__features(
    mb_pixels, pixel_len, color_space_tag, bins,
    mb_hist_l, mb_hist_a, mb_hist_b,
    mb_moments, mb_hist_len, mb_moments_len
  );

  /* Copy output data back to Rust buffers before freeing.
   * mengxi_extract_grading_features returns 0 on success (not pixel count). */
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
  Moonbit_object_header(mb_query)->rc += 8;
  Moonbit_object_header(mb_candidate)->rc += 8;
  Moonbit_object_header(mb_out)->rc += 8;

  /* Call MoonBit function */
  int32_t result = _M0FP216mengxi_2dmoonbit3lib31mengxi__bhattacharyya__distance(
    mb_query, mb_candidate, hist_len, channels, mb_out
  );

  /* Copy output back to Rust buffer */
  if (result >= 0) {
    memcpy(out_score, mb_out, 1 * sizeof(double));
  }

  /* Free MoonBit arrays (reverse order) */
  moonbit_drop_object(mb_out);
  moonbit_drop_object(mb_candidate);
  moonbit_drop_object(mb_query);

  return result;
}
