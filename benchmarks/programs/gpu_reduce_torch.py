# The gpu_reduce benchmark with PyTorch on the GPU: the same 1000 rounds of a whole
# sum and a row sum over a float32 matrix that stays on the device, the row sums
# brought back to the host each round as Neuro's are, and the same checksum.

import torch

n = 2048
i = torch.arange(n).reshape(-1, 1)
j = torch.arange(n).reshape(1, -1)
x = ((i * 7 + j * 3) % 4).float().cuda()
checksum = 0
for r in range(1000):
    x[r, r] = 3.0
    checksum += int(x.sum())
    rows = x.sum(1).cpu()
    checksum += int(rows[r])
print(f"checksum = {checksum}")
