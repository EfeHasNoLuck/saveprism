#!/usr/bin/env python3
"""Writes synthetic RPG Maker XP / VX / VX Ace saves as Ruby Marshal 4.8 data.

Written from the public format description (doc/language/marshal.rdoc in the Ruby repository)
and public knowledge of what RPG Maker stores in a save. Ruby is not used and nothing is
executed: this script only emits bytes. The data is invented.

    python fixtures/tools/marshal/make_marshal.py

writes
    fixtures/synthetic/rpgmaker-vxace/Save01.rvdata2  a VX Ace save (header hash, contents hash)
    fixtures/synthetic/rpgmaker-vxace/Save02.rvdata2  the same plus script data with rarer types
    fixtures/synthetic/rpgmaker-vxace/Save03.rvdata2  stat bonuses (@param_plus), experience of two
                                                      classes, an actor slot that is an object link
    fixtures/synthetic/rpgmaker-xp/Save1.rxdata       an XP save (one dump per game object, Ruby 1.8)
    fixtures/synthetic/rpgmaker-xp/Save2.rxdata       the same objects in another order (Game_Party
                                                      before Game_Actors, Game_System last)
    fixtures/synthetic/rpgmaker-vx/Save1.rvdata       a VX save (one dump per game object, Ruby 1.8)
    fixtures/synthetic/rpgmaker-*/Data/               a small game database for each engine (Items,
                                                      Weapons, Armors, System): the names of the saves

Like Ruby, the writer stores an object seen before as a link ("@", by Python object identity)
and a symbol seen before as a symbol link (";").
"""

import math
import os
import struct
from decimal import Decimal

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.normpath(os.path.join(HERE, "..", ".."))


# ----- Ruby values ------------------------------------------------------------------------------


class Sym:
    """A Symbol. `enc` "utf8" writes it as I: with :E => true (Ruby 1.9, non-ASCII names)."""

    def __init__(self, name, enc=None):
        self.name = name
        self.enc = enc


class RStr:
    """A String. `enc`: "utf8" (:E => true), "ascii" (:E => false), None (no I wrapper)."""

    def __init__(self, data, enc="utf8"):
        self.data = data.encode("utf-8") if isinstance(data, str) else bytes(data)
        self.enc = enc


class Flt:
    """A Float with an identity (the same Flt twice gives a link). `text` overrides the text."""

    def __init__(self, value, text=None):
        self.value = value
        self.text = text


class Big:
    """An Integer with an identity, written as a Bignum."""

    def __init__(self, value):
        self.value = value


class Obj:
    def __init__(self, cls, ivars):
        self.cls = cls
        self.ivars = ivars  # list of (name, value)


class RStruct:
    def __init__(self, cls, members):
        self.cls = cls
        self.members = members


class UserDef:
    """_dump data (Table, Color, Tone)."""

    def __init__(self, cls, data):
        self.cls = cls
        self.data = data


class UserMarshal:
    def __init__(self, cls, value):
        self.cls = cls
        self.value = value


class Extended:
    def __init__(self, module, value):
        self.module = module
        self.value = value


class UserClass:
    def __init__(self, cls, value):
        self.cls = cls
        self.value = value


class Regexp:
    def __init__(self, source, options, enc="utf8"):
        self.source = source.encode("utf-8")
        self.options = options
        self.enc = enc


class ClassRef:
    def __init__(self, name, tag=b"c"):
        self.name = name
        self.tag = tag


class RHash:
    """A Hash as a list of pairs (keys may be lists); `default` makes it a hash with default."""

    _NONE = object()

    def __init__(self, pairs, default=_NONE):
        self.pairs = pairs
        self.default = default


class WithIvars:
    """An I wrapper with other instance variables than :E (for example :encoding)."""

    def __init__(self, value, ivars):
        self.value = value
        self.ivars = ivars


def table(xsize, ysize, zsize, values):
    """RGSS Table._dump: dimensions, sizes, element count, then int16 values."""
    dim = 1 if ysize == 1 and zsize == 1 else (2 if zsize == 1 else 3)
    count = xsize * ysize * zsize
    assert len(values) == count
    data = struct.pack("<5i", dim, xsize, ysize, zsize, count)
    data += struct.pack("<%dh" % count, *values)
    return UserDef("Table", data)


def color(red, green, blue, alpha):
    return UserDef("Color", struct.pack("<4d", red, green, blue, alpha))


def tone(red, green, blue, gray):
    return UserDef("Tone", struct.pack("<4d", red, green, blue, gray))


# ----- writer -----------------------------------------------------------------------------------


def ruby19_float(value):
    """Float text of Ruby 1.9 and later: shortest digits, exponent form when far from them."""
    if math.isnan(value):
        return "nan"
    if math.isinf(value):
        return "-inf" if value < 0 else "inf"
    if value == 0:
        return "-0" if math.copysign(1.0, value) < 0 else "0"
    sign = "-" if value < 0 else ""
    digits_tuple = Decimal(repr(abs(value))).normalize().as_tuple()
    digits = "".join(str(d) for d in digits_tuple.digits)
    point = len(digits) + digits_tuple.exponent
    if point < -3 or point > len(digits):
        text = digits[0] + ("." + digits[1:] if len(digits) > 1 else "") + "e%d" % (point - 1)
    elif point > 0:
        text = digits[:point] + ("." + digits[point:] if len(digits) > point else "")
    else:
        text = "0." + "0" * (-point) + digits
    return sign + text


def ruby18_float(value):
    """Float text of Ruby 1.8 (RGSS1/RGSS2): %.16g (no extra mantissa bytes here)."""
    if math.isnan(value):
        return "nan"
    if math.isinf(value):
        return "-inf" if value < 0 else "inf"
    if value == 0:
        return "-0" if math.copysign(1.0, value) < 0 else "0"
    return "%.16g" % value


def long_bytes(n):
    """Marshal's packed integer ("long")."""
    if n == 0:
        return b"\x00"
    if 0 < n < 123:
        return bytes([n + 5])
    if -124 < n < 0:
        return bytes([(n - 5) & 0xFF])
    out = bytearray()
    x = n
    for count in range(1, 5):
        out.append(x & 0xFF)
        x >>= 8
        if (n >= 0 and x == 0) or (n < 0 and x == -1):
            head = count if n >= 0 else (256 - count)
            return bytes([head]) + bytes(out)
    raise ValueError("long out of range: %d" % n)


class Dumper:
    def __init__(self, float_text):
        self.float_text = float_text
        self.out = bytearray()
        self.symbols = {}
        self.objects = {}
        self.keep = []

    def dump(self, value):
        self.out += b"\x04\x08"
        self.symbols = {}
        self.objects = {}
        self.value(value)

    def raw(self, data):
        self.out += data

    def long(self, n):
        self.out += long_bytes(n)

    def bytes_(self, data):
        self.long(len(data))
        self.out += data

    def symbol(self, sym):
        if isinstance(sym, str):
            sym = Sym(sym)
        if sym.name in self.symbols:
            self.out += b";"
            self.long(self.symbols[sym.name])
            return
        self.symbols[sym.name] = len(self.symbols)
        if sym.enc:
            self.out += b"I:"
            self.bytes_(sym.name.encode("utf-8"))
            self.encoding(sym.enc)
        else:
            self.out += b":"
            self.bytes_(sym.name.encode("utf-8"))

    def encoding(self, enc):
        self.long(1)
        self.symbol("E")
        self.out += b"T" if enc == "utf8" else b"F"

    def remember(self, value):
        """Returns True (and writes a link) when the object was written before."""
        key = id(value)
        if key in self.objects:
            self.out += b"@"
            self.long(self.objects[key])
            return True
        self.objects[key] = len(self.objects)
        self.keep.append(value)
        return False

    def anonymous(self):
        """A new object that can never be linked (a plain Python float or big int)."""
        token = object()
        self.objects[id(token)] = len(self.objects)
        self.keep.append(token)

    def bignum(self, n):
        self.out += b"l"
        self.out += b"-" if n < 0 else b"+"
        magnitude = abs(n).to_bytes((abs(n).bit_length() + 7) // 8 or 1, "little")
        if len(magnitude) % 2:
            magnitude += b"\x00"
        self.long(len(magnitude) // 2)
        self.out += magnitude

    def value(self, v):
        if v is None:
            self.out += b"0"
        elif v is True:
            self.out += b"T"
        elif v is False:
            self.out += b"F"
        elif isinstance(v, int):
            if -(1 << 30) <= v < (1 << 30):
                self.out += b"i"
                self.long(v)
            else:
                self.anonymous()
                self.bignum(v)
        elif isinstance(v, float):
            self.anonymous()
            self.out += b"f"
            self.bytes_(self.float_text(v).encode("ascii"))
        elif isinstance(v, Sym):
            self.symbol(v)
        elif self.remember(v):
            return
        elif isinstance(v, Big):
            self.bignum(v.value)
        elif isinstance(v, Flt):
            self.out += b"f"
            self.bytes_((v.text or self.float_text(v.value)).encode("ascii"))
        elif isinstance(v, RStr):
            if v.enc:
                self.out += b"I"
            self.out += b'"'
            self.bytes_(v.data)
            if v.enc:
                self.encoding(v.enc)
        elif isinstance(v, Regexp):
            if v.enc:
                self.out += b"I"
            self.out += b"/"
            self.bytes_(v.source)
            self.out += bytes([v.options & 0xFF])
            if v.enc:
                self.encoding(v.enc)
        elif isinstance(v, list):
            self.out += b"["
            self.long(len(v))
            for item in v:
                self.value(item)
        elif isinstance(v, RHash):
            with_default = v.default is not RHash._NONE
            self.out += b"}" if with_default else b"{"
            self.long(len(v.pairs))
            for key, item in v.pairs:
                self.value(key)
                self.value(item)
            if with_default:
                self.value(v.default)
        elif isinstance(v, Obj):
            self.out += b"o"
            self.symbol(v.cls)
            self.long(len(v.ivars))
            for name, item in v.ivars:
                self.symbol(name)
                self.value(item)
        elif isinstance(v, RStruct):
            self.out += b"S"
            self.symbol(v.cls)
            self.long(len(v.members))
            for name, item in v.members:
                self.symbol(name)
                self.value(item)
        elif isinstance(v, UserDef):
            # _dump data is numbered after its bytes: take the index back and add it at the end.
            index = self.objects.pop(id(v))
            self.out += b"u"
            self.symbol(v.cls)
            self.bytes_(v.data)
            self.objects[id(v)] = index
        elif isinstance(v, UserMarshal):
            self.out += b"U"
            self.symbol(v.cls)
            self.value(v.value)
        elif isinstance(v, ClassRef):
            self.out += v.tag
            self.bytes_(v.name.encode("utf-8"))
        elif isinstance(v, Extended):
            # The wrapper is not an object of its own; its value is.
            self.objects.pop(id(v))
            self.out += b"e"
            self.symbol(v.module)
            self.value(v.value)
        elif isinstance(v, UserClass):
            self.objects.pop(id(v))
            self.out += b"C"
            self.symbol(v.cls)
            self.value(v.value)
        elif isinstance(v, WithIvars):
            self.objects.pop(id(v))
            self.out += b"I"
            self.value(v.value)
            self.long(len(v.ivars))
            for name, item in v.ivars:
                self.symbol(name)
                self.value(item)
        else:
            raise TypeError("cannot write %r" % (v,))


# ----- VX Ace -----------------------------------------------------------------------------------


def audio(cls, name, volume=100, pitch=100):
    return Obj(cls, [("@name", RStr(name)), ("@volume", volume), ("@pitch", pitch)])


def base_item(class_name=None, item_id=0):
    cls = ClassRef(class_name) if class_name else None
    return Obj("Game_BaseItem", [("@class", cls), ("@item_id", item_id)])


def vxace_actor(actor_id, name, nickname, character_name, level, exp, param_plus=None, exp_by_class=None):
    """`param_plus` adds @param_plus (8 stat bonuses); `exp_by_class` replaces {actor_id => exp}."""
    actor = Obj(
        "Game_Actor",
        [
            ("@hp", 450 + actor_id),
            ("@mp", 80),
            ("@tp", 0.0),
            ("@states", []),
            ("@state_turns", RHash([])),
            ("@state_steps", RHash([])),
            ("@buffs", [0] * 8),
            ("@buff_turns", RHash([])),
            ("@actor_id", actor_id),
            ("@name", RStr(name)),
            ("@nickname", RStr(nickname)),
            ("@character_name", character_name),
            ("@character_index", actor_id - 1),
            ("@face_name", RStr("Actor1")),
            ("@face_index", actor_id - 1),
            ("@class_id", actor_id),
            ("@level", level),
            ("@action_input_index", 0),
            ("@last_skill", base_item()),
            ("@exp", RHash(exp_by_class or [(actor_id, exp)])),
            ("@equips", [base_item("RPG::Weapon", 1), base_item("RPG::Armor", 2), base_item()]),
            ("@skills", [3, 4]),
            ("@battler_name", RStr("")),
            ("@battler_hue", 0),
        ],
    )
    if param_plus is not None:
        # Game_BattlerBase sets @param_plus right after @hp, @mp and @tp.
        actor.ivars.insert(3, ("@param_plus", param_plus))
    return actor


def vxace_contents(extra=None, actors=None, system_extra=None):
    """`actors` replaces Game_Actors' @data; `system_extra` adds an instance variable to Game_System
    (the first object of the hash, so an object in it is written before the actors)."""
    hero_sprite = RStr("Actor1")  # shared by the actor and the player: written once, then linked
    if actors is None:
        actors = [
            None,
            vxace_actor(1, "Eric", "The Brave", hero_sprite, 12, 4200),
            vxace_actor(2, "Natalie", "Ünal's friend ✦ ナタリー", RStr("Actor1"), 11, 3900),
        ]
    screen = Obj(
        "Game_Screen",
        [
            ("@brightness", 255),
            ("@tone", tone(0.0, 0.0, 0.0, 0.0)),
            ("@tone_target", tone(-34.0, -34.0, 0.0, 170.0)),
            ("@tone_duration", 0),
            ("@flash_color", color(255.0, 255.0, 255.0, 0.0)),
            ("@flash_duration", 0),
            ("@shake_power", 0),
            ("@shake_speed", 0),
            ("@shake_duration", 0),
            ("@shake_direction", 1),
            ("@shake", 0.0),
            ("@weather_type", Sym("none")),
            ("@weather_power", 0),
            ("@weather_duration", 0),
            ("@pictures", Obj("Game_Pictures", [("@data", [])])),
        ],
    )
    game_map = Obj(
        "Game_Map",
        [
            ("@map_id", 3),
            ("@display_x", 3.5),
            ("@display_y", 0.0),
            ("@parallax_name", RStr("")),
            ("@map", Obj(
                "RPG::Map",
                [
                    ("@display_name", RStr("Harbor Town")),
                    ("@tileset_id", 1),
                    ("@width", 3),
                    ("@height", 2),
                    ("@bgm", audio("RPG::BGM", "Town1")),
                    ("@data", table(3, 2, 2, [1536, 1537, 1538, 2048, 2049, 2050, 0, 0, 0, 0, 0, 0])),
                    ("@events", RHash([])),
                ],
            )),
            ("@events", RHash([])),
            ("@scroll_rest", 0),
            ("@scroll_speed", 4),
            ("@need_refresh", False),
        ],
    )
    player = Obj(
        "Game_Player",
        [
            ("@id", 0),
            ("@x", 5),
            ("@y", 7),
            ("@real_x", 5.0),
            ("@real_y", 6.75),
            ("@direction", 2),
            ("@character_name", hero_sprite),
            ("@character_index", 0),
            ("@move_speed", 4),
            ("@through", False),
            ("@followers", Obj("Game_Followers", [("@visible", True), ("@gathering", False), ("@data", [])])),
            ("@vehicle_type", Sym("walk")),
        ],
    )
    system = Obj(
        "Game_System",
        [
            ("@save_disabled", False),
            ("@menu_disabled", False),
            ("@encounter_disabled", False),
            ("@formation_disabled", False),
            ("@battle_count", 7),
            ("@save_count", 2),
            ("@version_id", 87654321),
            ("@window_tone", tone(-34.0, 0.0, 68.0, 0.0)),
            ("@battle_bgm", audio("RPG::BGM", "Battle1")),
            ("@battle_end_me", audio("RPG::ME", "Victory1")),
            ("@saved_bgm", None),
            ("@frames_on_save", 302400),
            ("@bgm_on_save", audio("RPG::BGM", "Town1")),
            ("@bgs_on_save", audio("RPG::BGS", "", 80)),
        ],
    )
    if system_extra:
        system.ivars.append(system_extra)
    pairs = [
        (Sym("system"), system),
        (Sym("screen"), screen),
        (Sym("timer"), Obj("Game_Timer", [("@count", 0), ("@working", False)])),
        (Sym("message"), Obj(
            "Game_Message",
            [
                ("@texts", []),
                ("@choices", []),
                ("@face_name", RStr("")),
                ("@face_index", 0),
                ("@background", 0),
                ("@position", 2),
                ("@choice_cancel_type", 0),
                ("@choice_proc", None),
                ("@num_input_variable_id", 0),
                ("@num_input_digits_max", 0),
                ("@item_choice_variable_id", 0),
                ("@scroll_mode", False),
                ("@scroll_speed", 2),
                ("@scroll_no_fast", False),
                ("@visible", False),
            ],
        )),
        (Sym("switches"), Obj("Game_Switches", [("@data", [None, True, False, None, True])])),
        (Sym("variables"), Obj(
            "Game_Variables",
            [("@data", [None, 10, 250, -3, 1 << 40, -(1 << 35), 1.5, -0.25, 1e20, 1e-05, RStr("Hello"), 0])],
        )),
        (Sym("self_switches"), Obj(
            "Game_SelfSwitches",
            [("@data", RHash([([1, 2, RStr("A")], True), ([1, 3, RStr("B")], False)]))],
        )),
        (Sym("actors"), Obj("Game_Actors", [("@data", actors)])),
        (Sym("party"), Obj(
            "Game_Party",
            [
                ("@in_battle", False),
                ("@gold", 1250),
                ("@steps", 3456),
                ("@last_item", base_item()),
                ("@menu_actor_id", 1),
                ("@target_actor_id", 1),
                ("@actors", [1, 2]),
                ("@items", RHash([(1, 5), (2, 1)])),
                ("@weapons", RHash([(1, 1)])),
                ("@armors", RHash([])),
            ],
        )),
        (Sym("troop"), Obj("Game_Troop", [("@in_battle", False), ("@turn_count", 0), ("@names_count", RHash([]))])),
        (Sym("map"), game_map),
        (Sym("player"), player),
    ]
    if extra:
        pairs.append(extra)
    return RHash(pairs)


def vxace_header():
    return RHash(
        [
            (Sym("characters"), [[RStr("Actor1"), 0], [RStr("Actor1"), 1]]),
            (Sym("playtime_s"), RStr("01:24:00")),
        ]
    )


def script_data():
    """What a script might add to the contents hash: the rarer Marshal types."""
    shared_float = Flt(2.5)
    shared_big = Big(1 << 62)
    loop = []
    loop.append(loop)
    pairs = [
        (Sym("point"), RStruct("Point", [("x", 1), ("y", -2)])),
        (Sym("custom"), UserMarshal("Custom_Data", [1, RStr("two")])),
        (Sym("extended"), Extended("Comparable", Obj("Object", []))),
        (Sym("my_list"), UserClass("My_List", [1, 2, 3])),
        (Sym("pattern"), Regexp("ab+c", 1)),
        (Sym("module"), ClassRef("Kernel", b"m")),
        (Sym("class"), ClassRef("Game_Party")),
        (Sym("counts"), RHash([(Sym("slime"), 3)], 0)),
        (Sym("by_id"), RHash([(1, 2)], None)),
        (Sym("sjis"), WithIvars(RStr(b"\x83\x41\x83\x43", None), [("encoding", RStr("Shift_JIS", None))])),
        (Sym("ascii"), RStr("plain", "ascii")),
        (Sym("binary"), RStr(b"\x00\x01\xff", None)),
        (Sym("odd_float"), Flt(2.5, "2.50")),
        (Sym("specials"), [float("inf"), float("-inf"), float("nan"), -0.0, 0.0]),
        (Sym("名前", "utf8"), Sym("勇者", "utf8")),
        (Sym("loop"), loop),
        (Sym("floats"), [shared_float, shared_float]),
        (Sym("bigs"), [shared_big, shared_big]),
        (Sym("string_keys"), RHash([(RStr("key"), 1), (RStr("other"), 2)])),
    ]
    return (Sym("script_data"), RHash(pairs))


def vxace_save03_contents():
    """Stat bonuses, experience of two classes (the current class second) and a script that keeps
    the companion in Game_System: Natalie is written there, so her slot in Game_Actors is a link."""
    eric = vxace_actor(
        1, "Eric", "The Brave", RStr("Actor1"), 12, 4200,
        param_plus=[50, 10, 5, 4, 3, 2, 1, 0],
        exp_by_class=[(2, 100), (1, 4200)],
    )
    natalie = vxace_actor(2, "Natalie", "The Wise", RStr("Actor1"), 11, 3900, param_plus=[0] * 8)
    return vxace_contents(actors=[None, eric, natalie], system_extra=("@companion", natalie))


def vxace_base_item(cls, item_id, name):
    return Obj(
        cls,
        [
            ("@id", item_id),
            ("@name", RStr(name)),
            ("@icon_index", item_id),
            ("@description", RStr("")),
            ("@features", []),
            ("@note", RStr("")),
            ("@price", 10 * item_id),
        ],
    )


def vxace_system():
    return Obj(
        "RPG::System",
        [
            ("@game_title", RStr("Synthetic Ace")),
            ("@version_id", 87654321),
            ("@japanese", False),
            ("@party_members", [1, 2]),
            ("@currency_unit", RStr("G")),
            ("@elements", [None, RStr("Fire")]),
            ("@switches", [None, RStr("Door Open"), RStr("Lights Off"), RStr(""), RStr("Met Natalie")]),
            ("@variables", [None] + [RStr("") for _ in range(10)] + [RStr("Zero Counter")]),
        ],
    )


# ----- XP (Ruby 1.8: no encodings) --------------------------------------------------------------


def xp_audio(name, volume=100, pitch=100):
    return Obj("RPG::AudioFile", [("@name", RStr(name, None)), ("@volume", volume), ("@pitch", pitch)])


def xp_actor(actor_id, name, character_name):
    return Obj(
        "Game_Actor",
        [
            ("@battler_name", RStr(character_name, None)),
            ("@battler_hue", 0),
            ("@hp", 741),
            ("@sp", 534),
            ("@states", []),
            ("@states_turn", RHash([])),
            ("@maxhp_plus", 0),
            ("@maxsp_plus", 0),
            ("@str_plus", 0),
            ("@dex_plus", 0),
            ("@agi_plus", 0),
            ("@int_plus", 0),
            ("@hidden", False),
            ("@immortal", False),
            ("@damage", None),
            ("@critical", False),
            ("@animation_id", 0),
            ("@animation_hit", False),
            ("@white_flash", False),
            ("@blink", False),
            ("@current_action", Obj("Game_BattleAction", [("@speed", 0), ("@kind", 0), ("@basic", 3), ("@skill_id", 0), ("@item_id", 0), ("@target_index", -1), ("@forcing", False)])),
            ("@actor_id", actor_id),
            ("@name", RStr(name, None)),
            ("@character_name", RStr(character_name, None)),
            ("@character_hue", 0),
            ("@class_id", actor_id),
            ("@weapon_id", 1),
            ("@armor1_id", 0),
            ("@armor2_id", 0),
            ("@armor3_id", 13),
            ("@armor4_id", 0),
            ("@level", 7),
            ("@exp_list", [0, 0, 25, 65, 127, 215, 335, 490, 685]),
            ("@exp", 500),
            ("@skills", [1, 7]),
        ],
    )


def xp_dumps():
    aluxes = xp_actor(1, "Aluxes", "001-Fighter01")
    basil = xp_actor(2, "Basil", "002-Lancer01")
    return [
        [[RStr("001-Fighter01", None), 0], [RStr("002-Lancer01", None), 0]],
        123456,
        Obj(
            "Game_System",
            [
                ("@map_interpreter", None),
                ("@battle_interpreter", None),
                ("@timer", 0),
                ("@timer_working", False),
                ("@save_disabled", False),
                ("@menu_disabled", False),
                ("@encounter_disabled", False),
                ("@message_position", 2),
                ("@message_frame", 0),
                ("@save_count", 3),
                ("@magic_number", 2468135),
                ("@playing_bgm", xp_audio("013-Theme02")),
                ("@playing_bgs", None),
                ("@windowskin_name", RStr("001-Blue01", None)),
                ("@battle_bgm", xp_audio("001-Battle01")),
            ],
        ),
        Obj("Game_Switches", [("@data", [None, True, False, True])]),
        Obj("Game_Variables", [("@data", [None, 100, 0, -7, 1 << 33])]),
        Obj("Game_SelfSwitches", [("@data", RHash([([2, 4, RStr("A", None)], True)]))]),
        Obj(
            "Game_Screen",
            [
                ("@tone", tone(0.0, 0.0, 0.0, 0.0)),
                ("@tone_target", tone(0.0, 0.0, 0.0, 0.0)),
                ("@tone_duration", 0),
                ("@flash_color", color(0.0, 0.0, 0.0, 0.0)),
                ("@flash_duration", 0),
                ("@shake_power", 0),
                ("@shake_speed", 0),
                ("@shake_duration", 0),
                ("@shake_direction", 1),
                ("@shake", 0),
                ("@pictures", [None]),
                ("@weather_type", 0),
                ("@weather_max", 0.0),
            ],
        ),
        Obj("Game_Actors", [("@data", [None, aluxes, basil])]),
        Obj(
            "Game_Party",
            [
                ("@actors", [aluxes, basil]),
                ("@gold", 5000),
                ("@steps", 1234),
                ("@items", RHash([(1, 3), (4, 1)])),
                ("@weapons", RHash([(1, 1)])),
                ("@armors", RHash([])),
            ],
        ),
        Obj("Game_Troop", [("@enemies", [])]),
        Obj(
            "Game_Map",
            [
                ("@map_id", 1),
                ("@display_x", 0),
                ("@display_y", 384),
                ("@need_refresh", False),
                ("@events", RHash([])),
                ("@common_events", RHash([])),
                ("@fog_ox", 0.5),
                ("@fog_oy", -1.25),
                ("@fog_tone", tone(0.0, 0.0, 0.0, 0.0)),
            ],
        ),
        Obj(
            "Game_Player",
            [
                ("@id", 0),
                ("@x", 10),
                ("@y", 8),
                ("@real_x", 1280),
                ("@real_y", 1024),
                ("@direction", 2),
                ("@character_name", RStr("001-Fighter01", None)),
                ("@character_hue", 0),
            ],
        ),
    ]


def xp_dumps_reordered():
    """The XP objects of `xp_dumps` with `Game_Party` before `Game_Actors` and `Game_System` last,
    the way a game that adds its own classes can write the top-level list."""
    dumps = xp_dumps()
    header, objects = dumps[:2], dumps[2:]
    priority = ["Game_Party", "Game_Actors", "Game_Variables", "Game_Switches", "Game_System"]
    return header + sorted(
        objects,
        key=lambda value: priority.index(value.cls)
        if isinstance(value, Obj) and value.cls in priority
        else len(priority),
    )


def xp_base_item(cls, item_id, name):
    return Obj(
        cls,
        [
            ("@id", item_id),
            ("@name", RStr(name, None)),
            ("@icon_name", RStr("", None)),
            ("@description", RStr("", None)),
            ("@price", 10 * item_id),
        ],
    )


def xp_system():
    return Obj(
        "RPG::System",
        [
            ("@magic_number", 2468135),
            ("@party_members", [1, 2]),
            ("@elements", [RStr("", None), RStr("Fire", None)]),
            ("@switches", [None] + [RStr(name, None) for name in ["Gate Open", "Lamp Lit", "Boss Beaten"]]),
            ("@variables", [None] + [RStr(name, None) for name in ["Gold Found", "Steps Taken", "", "Big Number"]]),
            ("@windowskin_name", RStr("001-Blue01", None)),
        ],
    )


# ----- VX (Ruby 1.8: no encodings) --------------------------------------------------------------


def vx_audio(cls, name, volume=100, pitch=100):
    return Obj(cls, [("@name", RStr(name, None)), ("@volume", volume), ("@pitch", pitch)])


def vx_actor(actor_id, name, level, hp, mp, exp, plus):
    """`plus`: the six stat bonuses (max HP, max MP, attack, defense, spirit, agility)."""
    actor = Obj("Game_Actor", [])
    # The battle action points back at its battler: a link inside the same dump.
    action = Obj(
        "Game_BattleAction",
        [("@battler", actor), ("@speed", 0), ("@kind", 0), ("@basic", -1), ("@skill_id", 0), ("@item_id", 0),
         ("@target_index", -1), ("@forcing", False), ("@value", 0)],
    )
    maxhp_plus, maxmp_plus, atk_plus, def_plus, spi_plus, agi_plus = plus
    actor.ivars = [
        ("@battler_name", RStr("", None)),
        ("@battler_hue", 0),
        ("@hp", hp),
        ("@mp", mp),
        ("@action", action),
        ("@states", []),
        ("@state_turns", RHash([])),
        ("@hidden", False),
        ("@immortal", False),
        ("@maxhp_plus", maxhp_plus),
        ("@maxmp_plus", maxmp_plus),
        ("@atk_plus", atk_plus),
        ("@def_plus", def_plus),
        ("@spi_plus", spi_plus),
        ("@agi_plus", agi_plus),
        ("@animation_id", 0),
        ("@white_flash", False),
        ("@collapse", False),
        ("@actor_id", actor_id),
        ("@name", name),
        ("@character_name", RStr("Actor1", None)),
        ("@character_index", actor_id - 1),
        ("@face_name", RStr("Actor1", None)),
        ("@face_index", actor_id - 1),
        ("@class_id", actor_id),
        ("@weapon_id", 1),
        ("@armor1_id", 0),
        ("@armor2_id", 0),
        ("@armor3_id", 0),
        ("@armor4_id", 0),
        ("@level", level),
        ("@exp_list", [0, 0, 25, 65, 127, 215, 335]),
        ("@exp", exp),
        ("@skills", [1]),
    ]
    return actor


def vx_dumps():
    ralph = vx_actor(1, RStr("Ralph", None), 9, 612, 88, 2500, (50, 0, 5, 3, 0, 2))
    ulrika = vx_actor(2, RStr("Ulrika", None), 8, 480, 120, 2100, (0, 0, 0, 0, 0, 0))
    # A name in Latin-1, not UTF-8: it reads as bytes.
    bennett = vx_actor(3, RStr(b"Benn\xe9tt", None), 6, 390, 60, 900, (0, 20, 0, 0, 4, 0))
    return [
        [[RStr("Actor1", None), 0], [RStr("Actor1", None), 2]],
        654321,
        vx_audio("RPG::BGM", "Town1"),
        vx_audio("RPG::BGS", "", 80),
        Obj(
            "Game_System",
            [
                ("@timer", 0),
                ("@timer_working", False),
                ("@save_disabled", False),
                ("@menu_disabled", False),
                ("@encounter_disabled", False),
                ("@save_count", 4),
                ("@version_id", 13579),
                ("@battle_bgm", vx_audio("RPG::BGM", "Battle1")),
                ("@battle_end_me", vx_audio("RPG::ME", "Victory1")),
            ],
        ),
        Obj(
            "Game_Message",
            [
                ("@texts", []),
                ("@face_name", RStr("", None)),
                ("@face_index", 0),
                ("@background", 0),
                ("@position", 2),
                ("@visible", False),
            ],
        ),
        Obj("Game_Switches", [("@data", [None, False, True, None, True])]),
        Obj("Game_Variables", [("@data", [None, 30, 0, -12, RStr("note", None)])]),
        Obj("Game_SelfSwitches", [("@data", RHash([([1, 1, RStr("A", None)], True)]))]),
        Obj("Game_Actors", [("@data", [None, ralph, ulrika, bennett])]),
        Obj(
            "Game_Party",
            [
                ("@gold", 9000),
                ("@steps", 222),
                ("@last_item_id", 0),
                ("@last_actor_index", 0),
                ("@last_target_index", 0),
                ("@actors", [1, 3]),
                ("@items", RHash([(1, 3), (9, 2)])),
                ("@weapons", RHash([(2, 1)])),
                ("@armors", RHash([(5, 1)])),
            ],
        ),
        Obj("Game_Troop", [("@enemies", []), ("@turn_count", 0), ("@names_count", RHash([]))]),
        Obj(
            "Game_Map",
            [
                ("@map_id", 2),
                ("@display_x", 0),
                ("@display_y", 512),
                ("@need_refresh", False),
                ("@events", RHash([])),
                ("@common_events", RHash([])),
                ("@parallax_x", 0.5),
            ],
        ),
        Obj(
            "Game_Player",
            [
                ("@id", 0),
                ("@x", 4),
                ("@y", 9),
                ("@real_x", 1024),
                ("@real_y", 2304),
                ("@direction", 8),
                ("@character_name", RStr("Actor1", None)),
                ("@character_index", 0),
            ],
        ),
    ]


def vx_base_item(cls, item_id, name):
    return Obj(
        cls,
        [
            ("@id", item_id),
            ("@name", RStr(name, None)),
            ("@icon_index", item_id),
            ("@description", RStr("", None)),
            ("@note", RStr("", None)),
            ("@price", 10 * item_id),
        ],
    )


def vx_system():
    return Obj(
        "RPG::System",
        [
            ("@game_title", RStr("Synthetic VX", None)),
            ("@version_id", 13579),
            ("@party_members", [1, 3]),
            ("@elements", [None, RStr("Fire", None)]),
            ("@switches", [None] + [RStr(name, None) for name in ["Bell Rung", "", "", ""]]),
            ("@variables", [None] + [RStr(name, None) for name in ["Coins", "Day", "", ""]]),
        ],
    )


# ----- game databases ---------------------------------------------------------------------------


def database(make, cls, names, count):
    """[nil, entry 1, ..., entry count], as in Items/Weapons/Armors; ids without a name get ""."""
    return [None] + [make(cls, item_id, names.get(item_id, "")) for item_id in range(1, count + 1)]


def write_data(folder, extension, float_text, make, items, weapons, armors, system):
    for name, value in [
        ("Items", database(make, "RPG::Item", items, max(items))),
        ("Weapons", database(make, "RPG::Weapon", weapons, max(weapons))),
        ("Armors", database(make, "RPG::Armor", armors, max(armors))),
        ("System", system),
    ]:
        dumper = Dumper(float_text)
        dumper.dump(value)
        write(os.path.join("synthetic", folder, "Data", "%s.%s" % (name, extension)), bytes(dumper.out))


def write(relative, data):
    path = os.path.join(FIXTURES, relative)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as handle:
        handle.write(data)
    print("%s (%d bytes)" % (relative.replace(os.sep, "/"), len(data)))


def main():
    ace = Dumper(ruby19_float)
    ace.dump(vxace_header())
    ace.dump(vxace_contents())
    write(os.path.join("synthetic", "rpgmaker-vxace", "Save01.rvdata2"), bytes(ace.out))

    ace = Dumper(ruby19_float)
    ace.dump(vxace_header())
    ace.dump(vxace_contents(script_data()))
    write(os.path.join("synthetic", "rpgmaker-vxace", "Save02.rvdata2"), bytes(ace.out))

    ace = Dumper(ruby19_float)
    ace.dump(vxace_header())
    ace.dump(vxace_save03_contents())
    write(os.path.join("synthetic", "rpgmaker-vxace", "Save03.rvdata2"), bytes(ace.out))

    xp = Dumper(ruby18_float)
    for value in xp_dumps():
        xp.dump(value)
    write(os.path.join("synthetic", "rpgmaker-xp", "Save1.rxdata"), bytes(xp.out))

    xp = Dumper(ruby18_float)
    for value in xp_dumps_reordered():
        xp.dump(value)
    write(os.path.join("synthetic", "rpgmaker-xp", "Save2.rxdata"), bytes(xp.out))

    vx = Dumper(ruby18_float)
    for value in vx_dumps():
        vx.dump(value)
    write(os.path.join("synthetic", "rpgmaker-vx", "Save1.rvdata"), bytes(vx.out))

    write_data(
        "rpgmaker-vxace", "rvdata2", ruby19_float, vxace_base_item,
        {1: "Healing Herb", 2: "Ether Drop"}, {1: "Épée courte"}, {1: "Leather Cap"}, vxace_system(),
    )
    write_data(
        "rpgmaker-xp", "rxdata", ruby18_float, xp_base_item,
        {1: "Herb Tonic", 2: "Clear Water", 4: "Phoenix Ash"}, {1: "Bronze Sword"}, {1: "Cotton Shirt"},
        xp_system(),
    )
    write_data(
        "rpgmaker-vx", "rvdata", ruby18_float, vx_base_item,
        {1: "Herb", 9: "Smelling Salts"}, {1: "Club", 2: "Long Spear"}, {5: "Round Shield"}, vx_system(),
    )


if __name__ == "__main__":
    main()
