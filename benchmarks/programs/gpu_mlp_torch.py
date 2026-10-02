# The gpu_mlp benchmark with PyTorch on the GPU: the same two-layer forward pass,
# twenty rounds, and the same data movement. The weights and biases move to the GPU
# once, each batch is staged from the host, and its outputs come back for the
# checksum. Inference mode, and full IEEE float32 matmuls (no TF32), as Neuro's are.

import torch

i = torch.arange(1024).reshape(-1, 1)
x = ((i * 7 + torch.arange(1024) * 3) % 4).float()
w1 = ((i * 5 + torch.arange(1024) * 11) % 5 - 2).float().cuda()
w2 = ((i * 3 + torch.arange(256) * 13) % 3 - 1).float().cuda()
b1 = (torch.arange(1024) % 7 - 3).float().cuda()
b2 = (torch.arange(256) % 5 - 2).float().cuda()
checksum = 0
with torch.inference_mode():
    for r in range(20):
        x[r, r] += 1.0
        out = (torch.relu(x.cuda() @ w1 + b1) @ w2 + b2).cpu()
        checksum += int(out.to(torch.int64).sum())
print(f"checksum = {checksum}")
