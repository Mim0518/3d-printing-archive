import {
  Alert,
  Anchor,
  Badge,
  Button,
  Card,
  ColorSwatch,
  Group,
  Loader,
  Modal,
  SimpleGrid,
  Stack,
  Table,
  Tabs,
  Text,
  Title,
  Tooltip,
} from '@mantine/core'
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate, useSearchParams } from 'react-router'

import { api } from '../api'
import { FormImpresora, FormMaterial, FormPerfilNuevo } from '../componentes/FormulariosTaller'
import { corto, dinero, n, resumenImpresora, resumenPerfil } from '../formato'
import { nombreMaterial, useTaller } from '../opciones'
import type { Impresora, Importables, Material, ResumenTaller } from '../tipos'

/** Lo que hay para importar de los archivos de perfiles y todavía no está en la app. */
function AvisoImportar() {
  const q = useQuery({ queryKey: ['importables'], queryFn: () => api<Importables>('/taller/importar') })
  // solo lo que se sugiere importar (lo de archivos más viejos no insiste)
  const pendientes = q.data?.propuestas.filter((p) => p.ya === null && p.importable && p.marcada) ?? []
  if (q.isPending || pendientes.length === 0) return null
  const cuenta = (c: string) => pendientes.filter((p) => p.clase === c).length
  return (
    <Alert color="blue" variant="light" title="Hay perfiles en tus archivos que todavía no están aquí">
      <Text size="sm">
        En Photon Workshop, Orca y Lychee encontré{' '}
        {[
          [cuenta('impresora'), 'impresora', 'impresoras'],
          [cuenta('material'), 'material', 'materiales'],
          [cuenta('perfil'), 'perfil', 'perfiles'],
        ]
          .filter(([c]) => (c as number) > 0)
          .map(([c, uno, varios]) => `${n(c as number)} ${c === 1 ? uno : varios}`)
          .join(', ')}
        . Revísalos antes de importarlos.
      </Text>
      <Button size="xs" mt="xs" component={Link} to="/impresoras/importar">
        Revisar e importar
      </Button>
    </Alert>
  )
}

function TarjetaImpresora({ i }: { i: Impresora }) {
  return (
    <Card withBorder component={Link} to={`/impresoras/${i.id}`} style={{ textDecoration: 'none' }}>
      <Group justify="space-between" align="flex-start" wrap="nowrap">
        <Text fw={600}>{i.nombre}</Text>
        <Group gap={4}>
          <Badge variant="light" color={i.tecnologia === 'fdm' ? 'orange' : 'teal'}>
            {i.tecnologia === 'fdm' ? 'FDM' : 'resina'}
          </Badge>
          {!i.en_uso && (
            <Badge variant="outline" color="gray">
              ya no se usa
            </Badge>
          )}
        </Group>
      </Group>
      <Text size="sm" c="dimmed">
        {resumenImpresora(i)}
      </Text>
      <Text size="sm" mt="xs">
        {i.perfil ? (
          <>
            Perfil normal: <b>{i.perfil}</b>
          </>
        ) : (
          <Text span c="dimmed" inherit>
            Sin perfil normal
          </Text>
        )}
      </Text>
      <Text size="xs" c="dimmed">
        {n(i.n_perfiles)} {i.n_perfiles === 1 ? 'perfil' : 'perfiles'} · {n(i.n_placas)} placas listas para ella ·{' '}
        {n(i.n_laminados)} laminados
      </Text>
    </Card>
  )
}

function ImpresorasTab() {
  const { impresoras } = useTaller()
  const navegar = useNavigate()
  const resumen = useQuery({ queryKey: ['taller-resumen'], queryFn: () => api<ResumenTaller>('/taller/resumen') })
  const [nueva, setNueva] = useState<Partial<Impresora> | null>(null)
  return (
    <Stack>
      <Group>
        <Button size="xs" variant="light" onClick={() => setNueva({})}>
          Nueva impresora
        </Button>
      </Group>
      {impresoras.length === 0 && <Text c="dimmed">Todavía no hay impresoras.</Text>}
      <SimpleGrid cols={{ base: 1, sm: 2, lg: 3 }}>
        {impresoras.map((i) => (
          <TarjetaImpresora key={i.id} i={i} />
        ))}
      </SimpleGrid>
      {resumen.data?.laminados_sin_impresora.map((l) => (
        <Alert key={`${l.res_x}x${l.res_y}`} color="gray" variant="light">
          <Group justify="space-between">
            <Text size="sm">
              {n(l.laminados)} laminados son para una pantalla de {l.res_x}×{l.res_y}
              {l.maquina ? ` («${l.maquina}»)` : ''}, que no es de ninguna impresora de la app. Si son de una que tienes,
              agrégala; si vinieron así de un estudio, no hace falta.
            </Text>
            <Button
              size="xs"
              variant="default"
              onClick={() =>
                setNueva({ nombre: l.maquina ?? '', tecnologia: 'resina', res_x: Math.min(l.res_x, l.res_y), res_y: Math.max(l.res_x, l.res_y) })
              }
            >
              Agregarla
            </Button>
          </Group>
        </Alert>
      ))}
      <Modal opened={nueva !== null} onClose={() => setNueva(null)} title="Nueva impresora" size="lg">
        {nueva && <FormImpresora inicial={nueva} onListo={(id) => navegar(`/impresoras/${id}`)} />}
      </Modal>
    </Stack>
  )
}

function MaterialesTab() {
  const { materiales } = useTaller()
  const [editando, setEditando] = useState<Partial<Material> | null>(null)
  const filas = (tipo: Material['tipo']) => materiales.filter((m) => m.tipo === tipo)
  return (
    <Stack>
      <Group>
        <Button size="xs" variant="light" onClick={() => setEditando({ tipo: 'resina' })}>
          Nueva resina
        </Button>
        <Button size="xs" variant="light" onClick={() => setEditando({ tipo: 'filamento' })}>
          Nuevo filamento
        </Button>
      </Group>
      {(['resina', 'filamento'] as const).map((tipo) => (
        <Card withBorder key={tipo} padding="xs">
          <Text fw={500} mb="xs">
            {tipo === 'resina' ? 'Resinas' : 'Filamentos'}
          </Text>
          {filas(tipo).length === 0 ? (
            <Text size="sm" c="dimmed">
              Ninguno todavía.
            </Text>
          ) : (
            <Table.ScrollContainer minWidth={640}>
              <Table highlightOnHover>
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>Material</Table.Th>
                    <Table.Th>Presentación</Table.Th>
                    <Table.Th ta="right">Precio</Table.Th>
                    <Table.Th ta="right">Por {tipo === 'resina' ? 'ml' : 'g'}</Table.Th>
                    <Table.Th ta="right">Densidad</Table.Th>
                    <Table.Th ta="right">Queda</Table.Th>
                    <Table.Th>Usado en</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {filas(tipo).map((m) => (
                    <Table.Tr key={m.id} style={{ cursor: 'pointer', opacity: m.en_uso ? 1 : 0.55 }} onClick={() => setEditando(m)}>
                      <Table.Td>
                        <Group gap="xs" wrap="nowrap">
                          {m.color_hex && <ColorSwatch color={m.color_hex} size={14} />}
                          <div>
                            <Text size="sm">{nombreMaterial(m)}</Text>
                            <Text size="xs" c="dimmed">
                              {[m.subtipo, m.color, m.en_uso ? '' : 'ya no se usa'].filter(Boolean).join(' · ')}
                            </Text>
                          </div>
                        </Group>
                      </Table.Td>
                      <Table.Td>{m.presentacion ? `${corto(m.presentacion)} ${tipo === 'resina' ? 'ml' : 'g'}` : ''}</Table.Td>
                      <Table.Td ta="right">{m.precio != null ? dinero(m.precio) : <Text c="orange" size="sm">sin precio</Text>}</Table.Td>
                      <Table.Td ta="right">{dinero(m.precio_unidad, true)}</Table.Td>
                      <Table.Td ta="right">{m.densidad ? `${m.densidad} g/cm³` : ''}</Table.Td>
                      <Table.Td ta="right">{m.queda != null ? corto(m.queda) : ''}</Table.Td>
                      <Table.Td>
                        <Text size="xs" c="dimmed">
                          {[m.n_perfiles ? `${m.n_perfiles} perfiles` : '', m.n_placas ? `${m.n_placas} placas` : ''].filter(Boolean).join(' · ')}
                        </Text>
                      </Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </Table.ScrollContainer>
          )}
        </Card>
      ))}
      <Modal
        opened={editando !== null}
        onClose={() => setEditando(null)}
        title={editando?.id ? nombreMaterial(editando as Material) : 'Nuevo material'}
        size="lg"
      >
        {editando && <FormMaterial key={editando.id ?? 'nuevo'} inicial={editando} onListo={() => setEditando(null)} />}
      </Modal>
    </Stack>
  )
}

function PerfilesTab() {
  const { impresoras, perfiles } = useTaller()
  const navegar = useNavigate()
  const [nuevo, setNuevo] = useState(false)
  return (
    <Stack>
      <Group>
        <Button size="xs" variant="light" disabled={impresoras.length === 0} onClick={() => setNuevo(true)}>
          Nuevo perfil
        </Button>
      </Group>
      {impresoras.map((i) => {
        const suyos = perfiles.filter((p) => p.impresora_id === i.id)
        return (
          <Card withBorder key={i.id} padding="xs">
            <Anchor component={Link} to={`/impresoras/${i.id}`} fw={500} mb="xs">
              {i.nombre}
            </Anchor>
            {suyos.length === 0 ? (
              <Text size="sm" c="dimmed">
                Sin perfiles.
              </Text>
            ) : (
              <Table.ScrollContainer minWidth={560}>
                <Table highlightOnHover>
                  <Table.Tbody>
                    {suyos.map((p) => (
                      <Table.Tr key={p.id} style={{ cursor: 'pointer', opacity: p.en_uso ? 1 : 0.55 }} onClick={() => navegar(`/perfiles/${p.id}`)}>
                        <Table.Td>
                          <Group gap={6} wrap="nowrap">
                            {p.predeterminado && (
                              <Tooltip label="El que usa normalmente">
                                <Text c="yellow">★</Text>
                              </Tooltip>
                            )}
                            <div>
                              <Text size="sm">{p.nombre}</Text>
                              <Text size="xs" c="dimmed">
                                {p.material ?? 'cualquier material'}
                                {p.en_uso ? '' : ' · ya no se usa'}
                              </Text>
                            </div>
                          </Group>
                        </Table.Td>
                        <Table.Td>
                          <Text size="sm">{resumenPerfil(p) || <Text span c="dimmed" inherit>sin valores</Text>}</Text>
                        </Table.Td>
                        <Table.Td>
                          <Group gap={4} justify="flex-end">
                            <Badge size="xs" variant="outline" color="gray">
                              v{p.version}
                            </Badge>
                            {p.n_archivos > 0 && (
                              <Badge size="xs" variant="light" color="lime">
                                {p.n_archivos} {p.n_archivos === 1 ? 'archivo' : 'archivos'}
                              </Badge>
                            )}
                          </Group>
                        </Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              </Table.ScrollContainer>
            )}
          </Card>
        )
      })}
      <Modal opened={nuevo} onClose={() => setNuevo(false)} title="Nuevo perfil">
        {nuevo && <FormPerfilNuevo onListo={(id) => navegar(`/perfiles/${id}`)} />}
      </Modal>
    </Stack>
  )
}

export default function Taller() {
  const [params, setParams] = useSearchParams()
  const { impresoras, materiales, perfiles, cargando, error } = useTaller()
  const ver = params.get('ver') ?? 'impresoras'
  if (cargando) return <Loader />
  if (error) return <Alert color="red">{error.message}</Alert>
  return (
    <Stack maw={1200}>
      <Group justify="space-between" align="flex-end">
        <div>
          <Title order={2}>Impresoras y materiales</Title>
        <Text c="dimmed" size="sm">
          {n(impresoras.length)} impresoras · {n(materiales.length)} materiales · {n(perfiles.filter((p) => p.en_uso).length)} perfiles en
            uso
          </Text>
        </div>
        <Button size="xs" variant="default" component={Link} to="/impresoras/importar">
          Importar de tus archivos de perfiles
        </Button>
      </Group>
      <AvisoImportar />
      <Tabs value={ver} onChange={(v) => setParams(v && v !== 'impresoras' ? { ver: v } : {}, { replace: true })}>
        <Tabs.List>
          <Tabs.Tab value="impresoras">Impresoras</Tabs.Tab>
          <Tabs.Tab value="materiales">Materiales</Tabs.Tab>
          <Tabs.Tab value="perfiles">Perfiles</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="impresoras" pt="md">
          <ImpresorasTab />
        </Tabs.Panel>
        <Tabs.Panel value="materiales" pt="md">
          <MaterialesTab />
        </Tabs.Panel>
        <Tabs.Panel value="perfiles" pt="md">
          <PerfilesTab />
        </Tabs.Panel>
      </Tabs>
    </Stack>
  )
}
