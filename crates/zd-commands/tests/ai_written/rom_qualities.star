# Spry's ROM qualities after the ROM ash cut-off. A unit in a ROM coal section that isn't one of its coal plies
# (dilution, partings, coal lost into it) takes the section coal's moistures where it has none, is aligned to the
# coal's Mad where its parameter row says so (ash x (100 - coal Mad) / (100 - own Mad), RD divided by that factor),
# and takes the coal's ash where its own is negative. Every unit in a section takes the coal's length. Then ROM and
# air-dried-tonne densities are worked from the in situ density.
def formula(unit, p):
    coal = unit.rom_coal
    if coal != None:
        is_ply = unit.material.lower() == "coal" and unit.became == None
        if not is_ply:
            for m in ["mad", "mis", "mrom"]:
                if unit[m] == None and coal[m] != None:
                    unit[m] = coal[m]
            align = getattr(p, "align_dilution_mad", None)
            if align != None and align.lower() == "yes":
                if unit.mad != None and coal.mad != None and unit.mad != coal.mad:
                    factor = (100 - coal.mad) / (100 - unit.mad)
                    if unit.ash != None:
                        unit.ash = unit.ash * factor
                    if unit.rd != None:
                        unit.rd = unit.rd / factor
                    unit.mad = coal.mad
            if unit.ash != None and unit.ash < 0 and coal.ash != None:
                unit.ash = coal.ash
        if coal.length != None:
            unit.length = coal.length
    if unit.rd_is != None and unit.mis != None:
        if unit.mrom != None:
            unit.rd_rom = unit.rd_is * (100 - unit.mis) / (100 - unit.mrom)
        if unit.mad != None:
            unit.rd_adt = unit.rd_is * (100 - unit.mis) / (100 - unit.mad)
