# In situ, ROM and air-dried densities for units that have become nothing.
# Coal: in situ by Preston and Sanders (needs rd, mad, mis). Waste: rd taken to in situ through the Mad-to-Mis moisture factor.
def formula(unit):
    if unit.became != None:
        return
    coal = unit.material.lower() == "coal"
    if coal:
        missing = [n for n in ["rd", "mad", "mis"] if unit[n] == None]
        if missing:
            fail("coal with no " + " or ".join(missing))
        rd_is = in_situ_rd(unit.rd, unit.mad, unit.mis)
    elif unit.rd == None:
        return
    elif unit.mad != None and unit.mis != None:
        rd_is = unit.rd / moisture_factor(unit.mad, unit.mis)
    else:
        rd_is = unit.rd
    unit.rd_is = rd_is

    # ROM density: in situ x (100 - Mis) / (100 - Mrom); coal without both gets none, waste takes its in situ density.
    if unit.mis != None and unit.mrom != None:
        unit.rd_rom = rd_is * (100 - unit.mis) / (100 - unit.mrom)
    elif coal:
        unit.rd_rom = None
    else:
        unit.rd_rom = rd_is

    # Air-dried tonnes a cubic metre in situ: in situ x (100 - Mis) / (100 - Mad), else the in situ density.
    if unit.mad != None and unit.mis != None:
        unit.rd_adt = rd_is * (100 - unit.mis) / (100 - unit.mad)
    else:
        unit.rd_adt = rd_is
