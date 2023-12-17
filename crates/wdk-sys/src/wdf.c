#include "ntifs.h"
#include "ntddk.h"

// Instruct the compiler to keep all the Wdf* functions.
#undef FORCEINLINE
#define FORCEINLINE

#define WDF_STUB // Disable `WdfMinimumVersionRequired`
#include "wdf.h"
