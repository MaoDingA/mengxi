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
