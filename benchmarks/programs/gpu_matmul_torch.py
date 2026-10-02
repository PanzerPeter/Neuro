# The gpu_matmul benchmark with PyTorch on the GPU: same matrices, same five rounds,
# same data movement. The right operand moves to the GPU once, the left one is staged
# each round, and each product comes back to the host for the checksum. PyTorch's
# default float32 matmul is full IEEE precision (no TF32), as Neuro's is.

import torch

n = 2048
i = torch.arange(n).reshape(-1, 1)
j = torch.arange(n).reshape(1, -1)
a = ((i * 7 + j * 3) % 17).float()
b_gpu = ((i * 5 + j * 11) % 13).float().cuda()
checksum = 0
for r in range(5):
    a[r, r] += 1.0
    c = (a.cuda() @ b_gpu).cpu()
    checksum += int(c.to(torch.int64).sum())
print(f"checksum = {checksum}")
