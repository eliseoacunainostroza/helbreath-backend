BEGIN;

CREATE TEMP TABLE map_catalog_seed (
    code VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL
) ON COMMIT DROP;

INSERT INTO map_catalog_seed(code, name)
VALUES
    ('2ndmiddle', '2ndmiddle'),
    ('abaddon', 'Abaddon'),
    ('arebrk11', 'Arebrk11'),
    ('arebrk12', 'Arebrk12'),
    ('arebrk21', 'Arebrk21'),
    ('arebrk22', 'Arebrk22'),
    ('arefarm', 'Aresden Farm'),
    ('arejail', 'Aresden Jail'),
    ('aresden', 'Aresden City'),
    ('aresdend1', 'Aresden Dungeon 1'),
    ('areuni', 'Aresden Town Hall'),
    ('arewrhus', 'Aresden Warehouse'),
    ('asgarde', 'Asgarde'),
    ('bisle', 'Bisle'),
    ('bsmith_1', 'Bsmith 1'),
    ('bsmith_1f', 'Bsmith 1f'),
    ('bsmith_2', 'Bsmith 2'),
    ('bsmith_2f', 'Bsmith 2f'),
    ('bsmith_3', 'Bsmith 3'),
    ('btfield', 'Btfield'),
    ('catacombs', 'Catacombs'),
    ('cath_1', 'Cath 1'),
    ('cath_2', 'Cath 2'),
    ('cityhall_1', 'Cityhall 1'),
    ('cityhall_2', 'Cityhall 2'),
    ('cmdhall_1', 'Cmdhall 1'),
    ('cmdhall_2', 'Cmdhall 2'),
    ('default', 'Default Map'),
    ('dglv2', 'Dglv2'),
    ('dglv3', 'Dglv3'),
    ('dglv4', 'Dglv4'),
    ('druncncity', 'Druncncity'),
    ('elvbrk11', 'Elvbrk11'),
    ('elvbrk12', 'Elvbrk12'),
    ('elvbrk21', 'Elvbrk21'),
    ('elvbrk22', 'Elvbrk22'),
    ('elvfarm', 'Elvine Farm'),
    ('elvine', 'Elvine City'),
    ('elvined1', 'Elvine Dungeon 1'),
    ('elvjail', 'Elvine Jail'),
    ('elvuni', 'Elvine Town Hall'),
    ('elvwrhus', 'Elvine Warehouse'),
    ('erisnommire', 'Erisnommire'),
    ('fightzone1', 'Fightzone1'),
    ('fightzone2', 'Fightzone2'),
    ('fightzone3', 'Fightzone3'),
    ('fightzone4', 'Fightzone4'),
    ('fightzone5', 'Fightzone5'),
    ('fightzone6', 'Fightzone6'),
    ('fightzone7', 'Fightzone7'),
    ('fightzone8', 'Fightzone8'),
    ('fightzone9', 'Fightzone9'),
    ('gldhall_1', 'Gldhall 1'),
    ('gldhall_2', 'Gldhall 2'),
    ('godh', 'Godh'),
    ('gshop_1', 'Gshop 1'),
    ('gshop_1f', 'Gshop 1f'),
    ('gshop_2', 'Gshop 2'),
    ('gshop_2f', 'Gshop 2f'),
    ('gshop_3', 'Gshop 3'),
    ('hrampart', 'Hrampart'),
    ('huntzone1', 'Huntzone1'),
    ('huntzone2', 'Huntzone2'),
    ('huntzone3', 'Huntzone3'),
    ('huntzone4', 'Huntzone4'),
    ('icebound', 'Icebound'),
    ('inferniaa', 'Inferniaa'),
    ('inferniab', 'Inferniab'),
    ('lost', 'Lost'),
    ('maze', 'Maze'),
    ('middled1n', 'Middleland Mine'),
    ('middled1x', 'Middleland Dungeon'),
    ('middleland', 'Middleland'),
    ('procella', 'Procella'),
    ('qusmarsh', 'Qusmarsh'),
    ('resurr1', 'Resurr1'),
    ('resurr2', 'Resurr2'),
    ('stadium', 'Stadium'),
    ('toh1', 'Toh1'),
    ('toh2', 'Toh2'),
    ('toh3', 'Toh3'),
    ('wrhus_1', 'Wrhus 1'),
    ('wrhus_1f', 'Wrhus 1f'),
    ('wrhus_2', 'Wrhus 2'),
    ('wrhus_2f', 'Wrhus 2f'),
    ('wzdtwr_1', 'Wzdtwr 1'),
    ('wzdtwr_2', 'Wzdtwr 2')
ON CONFLICT (code) DO UPDATE
SET name = EXCLUDED.name;

UPDATE maps AS m
SET name = s.name
FROM map_catalog_seed AS s
WHERE m.code = s.code
  AND m.name IS DISTINCT FROM s.name;

WITH missing AS (
    SELECT s.code, s.name, ROW_NUMBER() OVER (ORDER BY s.code) AS rn
    FROM map_catalog_seed AS s
    LEFT JOIN maps AS m ON m.code = s.code
    WHERE m.code IS NULL
),
base AS (
    SELECT COALESCE(MAX(id), 0) AS max_id
    FROM maps
)
INSERT INTO maps(id, code, name, width, height, default_spawn_x, default_spawn_y, tick_ms)
SELECT
    base.max_id + missing.rn,
    missing.code,
    missing.name,
    2048,
    2048,
    100,
    100,
    50
FROM missing
CROSS JOIN base
ORDER BY missing.rn;

COMMIT;
