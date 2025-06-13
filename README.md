`bitfield-access` provides a simple and efficient macro-free way to read and write
bitfields in a raw memory buffer (anything that implements `AsRef<[u8]>`).

# Documentation

[bitfield-access docs](https://docs.rs/bitfield-access)

# Examples

Let's say you've received some bytes from the network and want to parse then as an IPv4 header
(see [IPv4 packet format](https://en.wikipedia.org/wiki/IPv4#Header)).
```text
0                   1                   2                   3
0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-------+-------+-----------+---+-------------------------------+
|Version|  IHL  |    DSCP   |ECN|         Total Length          |
+-------+-------+-----------+---+-----+-------------------------+
|        Identification         |Flags|     Fragment Offset     |
+---------------+---------------+-----+-------------------------+
|  Time to Live |   Protocol    |        Header Checksum        |
+---------------+---------------+-------------------------------+
|                     Source IP Address                         |
+---------------------------------------------------------------+
|                  Destination IP Address                       |
+---------------------------------------------------------------+
```

You might use the `BitfieldAccess` trait as follows:

```rust
use bitfield_access::BitfieldAccess;

// Simulated network buffer containing an IPv4 packet
let ipv4_packet: [u8; 20] = [
    0x45, 0x00, 0x00, 0x54, // Version(4), IHL(5), DSCP(0), ECN(0), Total Length(84)
    0x00, 0x00, 0x40, 0x00, // Identification, Flags, Fragment Offset
    0x40, 0x01, 0xf7, 0xb4, // TTL(64), Protocol(ICMP=1), Header Checksum
    0xc0, 0xa8, 0x00, 0x01, // Source IP (192.168.0.1)
    0xc0, 0xa8, 0x00, 0xc7, // Destination IP (192.168.0.199)
];

// Bitfields can be read into any unsigned integer primitive (u8, u16, u32, u64, u128)
let total_length: u16 = ipv4_packet.read_field(16..32); // bits 16 to 31
assert_eq!(total_length, 84);

// The return type must be known, but can be inferred by the compiler based on later usage
// or by using the turbofish operator.
let flags = ipv4_packet.read_field::<u8>(48..=50); // bits 48 to 50
assert_eq!(flags, 0b010);

// Manually shifting out bits of the IPv4 header isn't too difficult since most
// of the fields are byte aligned, but it can be error prone.
//
// BitfieldAccess supports arbitrary start and end bit indices, so long as they
// are within the bounds of the buffer and the resulting field can fit in the
// requested integer type.
let ecn_through_flags: u64 = ipv4_packet.read_field(14..=50);
// This is a contrived example, but some binary formats have fields that are not
// well aligned to byte boundaries.

// Writes work just like reads do
let mut ipv4_packet = ipv4_packet;
ipv4_packet.write_field(64..72, 21_u8); // set TTL to 21
assert_eq!(ipv4_packet[8], 21);
```

If the bit indices are outside the array bounds or the bitfield doesn't fit in the
requested integer type, it panics.

```rust should_panic
# use bitfield_access::BitfieldAccess;
# let ipv4_packet: [u8; 4] = [
#     0x45, 0x00, 0x00, 0x54, // Version(4), IHL(5), DSCP(0), ECN(0), Total Length(84)
# ];
// this will panic! 16-bit field does not fit in 8-bit integer
let total_length: u8 = ipv4_packet.read_field(16..32);
```

# Performance

When the bit indices are known at compile time, the compiler generates very efficient code.
For example, extracting the total length of the IPv4 packet in the example above
(bits `16..32`) compiles into only two instructions on my laptop:

```x86asm
movzx eax, word ptr [rdi + 2]
rol ax, 8
```

That's an easy case where the indices are byte aligned. If the compiler doesn't know the
length of the buffer it includes a quick bounds check (single `cmp` and jump instruction
in this case) as well.

For a more general case such as
reading bits `4..30` (spread across four bytes), the compiler generates:

```x86asm
movzx eax, byte ptr [rdi + 3]
shr eax, 2
movzx ecx, byte ptr [rdi + 2]
shl ecx, 6
or ecx, eax
movzx edx, byte ptr [rdi + 1]
shl edx, 14
or edx, ecx
movzx eax, byte ptr [rdi]
and eax, 15
shl eax, 22
or eax, edx
```

which is about as efficient as possible without going in to architecture and
alignment-specific optimizations.

If the bit indices are not known at compile time, the compiler must necessarily
generate a lot more generic code since it needs to handle arbitrary inputs, check
for more panic conditions, etc.
