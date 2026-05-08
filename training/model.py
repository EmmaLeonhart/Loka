"""Role-aware transformer for masked-triple prediction.

Sequence layout:
    [CLS] s_tokens... [SEP_S] p_tokens... [SEP_P] o_tokens... [SEP_O]

A role embedding (one of {special, S, P, O}) is added to each token's embedding,
so the model knows which slot every token belongs to. Token + position +
role embeddings are summed.

The classification head is tied to the input embedding for parameter efficiency.
"""
from __future__ import annotations

import torch
import torch.nn as nn

ROLE_SPECIAL, ROLE_S, ROLE_P, ROLE_O = 0, 1, 2, 3


class TripleTransformer(nn.Module):
    def __init__(
        self,
        vocab_size: int,
        d_model: int = 256,
        nhead: int = 8,
        num_layers: int = 4,
        max_len: int = 64,
        dropout: float = 0.1,
    ) -> None:
        super().__init__()
        self.token_emb = nn.Embedding(vocab_size, d_model, padding_idx=0)
        self.role_emb = nn.Embedding(4, d_model)
        self.pos_emb = nn.Embedding(max_len, d_model)
        self.emb_dropout = nn.Dropout(dropout)

        layer = nn.TransformerEncoderLayer(
            d_model=d_model,
            nhead=nhead,
            dim_feedforward=4 * d_model,
            dropout=dropout,
            activation="gelu",
            batch_first=True,
            norm_first=True,
        )
        self.encoder = nn.TransformerEncoder(layer, num_layers)
        self.norm = nn.LayerNorm(d_model)

        self.head = nn.Linear(d_model, vocab_size, bias=False)
        self.head.weight = self.token_emb.weight  # tie weights

    def forward(
        self,
        tokens: torch.Tensor,        # (B, T) long
        roles: torch.Tensor,         # (B, T) long
        attention_mask: torch.Tensor | None = None,  # (B, T) bool, True = attend
    ) -> torch.Tensor:               # (B, T, V)
        B, T = tokens.shape
        positions = torch.arange(T, device=tokens.device).unsqueeze(0).expand(B, T)
        x = self.token_emb(tokens) + self.role_emb(roles) + self.pos_emb(positions)
        x = self.emb_dropout(x)
        if attention_mask is not None:
            # PyTorch wants True = ignore for src_key_padding_mask
            x = self.encoder(x, src_key_padding_mask=~attention_mask)
        else:
            x = self.encoder(x)
        x = self.norm(x)
        return self.head(x)

    def num_parameters(self) -> int:
        return sum(p.numel() for p in self.parameters())
