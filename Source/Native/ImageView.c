#include <stdlib.h>
#include <nvCVImage.h>

// Image views describe memory owned by D3D11 / CUDA. The C ABI does not
// allocate or free GPU pixels via the C++ NvCVImage constructors.
NvCVImage* PaneAllocImages(unsigned count) { return calloc(count, sizeof(NvCVImage)); }
NvCVImage* PaneGetImage(NvCVImage* images, unsigned index) { return &images[index]; }
