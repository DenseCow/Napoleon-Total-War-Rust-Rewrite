# 0-E round N+2 raw evidence - PRIVATE FORK ONLY

Same warning as ../README.md: these are dumps of Creative Assembly's shipped UI bytecode
(data.pack, read-only) and must NOT be merged into the public repo.

Round N+2 disassembled four shipped .luac files to pin the shapes the wiring needed.
The disassembler was a temporary crates/ntw_script/examples/_scratch_luac.rs (deleted after
the dumps were taken). Files:

- luac__name_index.txt      - which .luac names each string (naval_recruitment_tab and
                              CampaignShipCard: NONE -- CONFIRMED UNKNOWN),
- luac__construction.txt    - ui/construction.luac: ResetConstructionPanel (61),
                              DemolishCurrentSelection (366), RepairCurrentSelection (383),
                              SelectPassiveConstructionSlotExclusive (622),
                              GenerateFortConstructionPanel (879),
- luac__buildingframe.txt   - ui/templates/template.buildingframe.luac: the frame's select
                              handler (417) - UpgradeFort/CancelFortRepair/CancelUpgradeFort,
- luac__building_information.txt - ui/campaign ui/building_information_scripts/
                              building_information.luac (36) - DemolishBuilding/DemolishFort,
- luac__army.txt            - ui/army.luac: GenerateArmyPanel (213) reads can_build_fort_status
                              and build_fort_cost, AbleToBuildFort (817), ShowArmyButtons (1058),
