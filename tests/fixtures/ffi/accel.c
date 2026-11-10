#include <stdlib.h>
typedef struct { float total; int calls; } Acc;
float accel_sum(const float* x, int n) { float s = 0; for (int i = 0; i < n; i++) s += x[i]; return s; }
void* accel_new(void) { Acc* a = calloc(1, sizeof(Acc)); return a; }
void accel_add(void* h, const float* x, int n) { Acc* a = h; a->total += accel_sum(x, n); a->calls++; }
double accel_total(void* h) { return ((Acc*)h)->total; }
void accel_free(void* h) { free(h); }
