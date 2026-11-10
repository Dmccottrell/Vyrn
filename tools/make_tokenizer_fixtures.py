"""Builds the tokenizer test fixtures with Hugging Face `tokenizers`.

Trains three small BPE tokenizers, one per supported family, and records
how the reference implementation encodes a set of test strings. The Rust
tests check that Vyrn produces exactly the same ids and decodes them back.

    pip install tokenizers==0.23.3
    python3 tools/make_tokenizer_fixtures.py
"""

import json
import os

from tokenizers import Regex, Tokenizer, decoders, models, normalizers, pre_tokenizers, trainers

OUT = os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures", "tokenizers")

CORPUS = [
    "The quick brown fox jumps over the lazy dog. It's 2024, and we're testing tokenizers!",
    "Machine learning models run on GPUs; game engines update entities every frame.",
    "Vyrn compiles to bytecode. Don't panic: the VM'll handle it, won't it? I'd say yes.",
    "Numbers like 12345, 3.14159 and 1000000 appear often in code and data.",
    "def main():\n    print('hello world')\n    return 0\n",
    "Café, naïve, résumé, 東京, Ελληνικά, emoji 🚀🔥 and symbols #@$%^&*()",
    "    indented lines\n\n\twith tabs and   multiple   spaces  \n",
] * 40

SAMPLES = [
    "Hello world",
    "It's a test, isn't it? We're sure THEY'LL agree.",
    "The year 2024 had 365 days and 12345678 seconds... roughly.",
    "def f(x):\n    return x * 2\n\n\nprint(f(21))",
    "Café 東京 🚀 naïve",
    "  leading and trailing spaces  ",
    "tabs\tand\nnewlines\r\nmixed",
    "<|endoftext|>Special tokens<|endoftext|> in the middle",
    "unseen words: xylophone quizzical zephyr",
    "",
    "a",
    "!!!???...",
]

SPECIAL = ["<|endoftext|>"]


def byte_level(name, split):
    tokenizer = Tokenizer(models.BPE())
    if split == "llama3":
        pattern = r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}{1,3}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+"
        tokenizer.pre_tokenizer = pre_tokenizers.Sequence(
            [
                pre_tokenizers.Split(Regex(pattern), behavior="isolated"),
                pre_tokenizers.ByteLevel(add_prefix_space=False, use_regex=False),
            ]
        )
    else:
        tokenizer.pre_tokenizer = pre_tokenizers.ByteLevel(add_prefix_space=False)
    tokenizer.decoder = decoders.ByteLevel()
    trainer = trainers.BpeTrainer(
        vocab_size=600,
        special_tokens=SPECIAL,
        initial_alphabet=pre_tokenizers.ByteLevel.alphabet(),
    )
    tokenizer.train_from_iterator(CORPUS, trainer)
    return tokenizer


def sentencepiece():
    tokenizer = Tokenizer(models.BPE(byte_fallback=True, unk_token="<unk>"))
    tokenizer.normalizer = normalizers.Sequence(
        [normalizers.Prepend("▁"), normalizers.Replace(" ", "▁")]
    )
    tokenizer.decoder = decoders.Sequence(
        [
            decoders.Replace("▁", " "),
            decoders.ByteFallback(),
            decoders.Fuse(),
            decoders.Strip(" ", 1, 0),
        ]
    )
    byte_tokens = [f"<0x{b:02X}>" for b in range(256)]
    trainer = trainers.BpeTrainer(
        vocab_size=900, special_tokens=["<unk>"] + SPECIAL + byte_tokens
    )
    tokenizer.train_from_iterator(CORPUS, trainer)
    return tokenizer


def main():
    os.makedirs(OUT, exist_ok=True)
    expected = {}
    for name, tokenizer in [
        ("gpt2", byte_level("gpt2", "gpt2")),
        ("llama3", byte_level("llama3", "llama3")),
        ("spm", sentencepiece()),
    ]:
        tokenizer.save(os.path.join(OUT, f"{name}.json"))
        cases = []
        for text in SAMPLES:
            ids = tokenizer.encode(text, add_special_tokens=False).ids
            cases.append({"text": text, "ids": ids, "decoded": tokenizer.decode(ids, skip_special_tokens=False)})
        expected[name] = cases
    with open(os.path.join(OUT, "expected.json"), "w", encoding="utf-8") as f:
        json.dump(expected, f, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
