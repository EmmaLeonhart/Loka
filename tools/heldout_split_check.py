"""Held-out check for link prediction: triples in normalized-wikidata v14-1M that
the v13 model never trained on, whose subject, predicate and object labels all
occur in the v13-500k corpus (the transductive setting).

Usage (corpus files from https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata,
tags v13-500k and v14-1M, file triples_normalized.txt):
    python tools/heldout_split_check.py V13_TRIPLES V14_TRIPLES
"""
import sys
from collections import Counter

v13_path, v14_path = sys.argv[1], sys.argv[2]


def read(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) == 3:
                yield tuple(parts)


train = set()
ents, preds = set(), set()
for s, p, o in read(v13_path):
    train.add((s, p, o))
    ents.add(s); ents.add(o); preds.add(p)
print("v13 triples (unique):", len(train), "entities/labels:", len(ents), "predicates:", len(preds))

n14 = 0
new = 0
trans = []
subj_new = 0
seen = set()
for t in read(v14_path):
    n14 += 1
    if t in train or t in seen:
        continue
    seen.add(t)
    new += 1
    s, p, o = t
    if s in ents and o in ents and p in preds:
        trans.append(t)
    if s not in ents:
        subj_new += 1
print("v14 lines:", n14, "unique triples not in v13:", new, "with subject label unseen in v13:", subj_new)
print("transductive held-out (s,p,o labels all seen in v13):", len(trans))
pc = Counter(p for _, p, _ in trans)
print("distinct predicates in held-out:", len(pc))
print("top predicates:", pc.most_common(15))
# Filtered-setting sanity: how many held-out (s,p) already have an object in train?
sp_train = set((s, p) for s, p, _ in train)
print("held-out whose (s,p) has another object in v13 train:", sum((s, p) in sp_train for s, p, _ in trans))
