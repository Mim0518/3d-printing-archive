import {
  Alert,
  Anchor,
  Badge,
  Breadcrumbs,
  Button,
  Card,
  Group,
  Loader,
  NumberInput,
  Select,
  Stack,
  Switch,
  Table,
  Text,
  TextInput,
  Textarea,
  Title,
} from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'

import { api } from '../api'
import { CamposPerfil } from '../componentes/FormulariosTaller'
import { COLOR_TIPO, corto, cuando, tamano } from '../formato'
import { aNumero, nombreMaterial, useRefrescarTaller, useTaller } from '../opciones'
import type { ArchivoEncontrado, Campo, Parametros, PerfilFicha, VersionPerfil } from '../tipos'

function valor(c: Campo | undefined, v: number | string | undefined): string {
  if (v === undefined) return '—'
  return typeof v === 'number' ? `${corto(v, 3)}${c?.unidad ? ` ${c.unidad}` : ''}` : v
}

/** Qué cambió de una versión a la siguiente: «Exposición 2 s → 2.2 s». */
function diferencias(
  antes: { capa_mm: number | null; material: string | null; parametros: Parametros },
  despues: { capa_mm: number | null; material: string | null; parametros: Parametros },
  campos: Campo[],
): string[] {
  const d: string[] = []
  if (antes.material !== despues.material) d.push(`Material: ${antes.material ?? 'cualquiera'} → ${despues.material ?? 'cualquiera'}`)
  if (antes.capa_mm !== despues.capa_mm) d.push(`Capa: ${corto(antes.capa_mm, 3) || '—'} → ${corto(despues.capa_mm, 3) || '—'} mm`)
  const claves = new Set([...Object.keys(antes.parametros), ...Object.keys(despues.parametros)])
  for (const k of claves) {
    if (antes.parametros[k] !== despues.parametros[k]) {
      const c = campos.find((x) => x.clave === k)
      d.push(`${c?.etiqueta ?? k}: ${valor(c, antes.parametros[k])} → ${valor(c, despues.parametros[k])}`)
    }
  }
  return d
}

function Versiones({ p, campos }: { p: PerfilFicha; campos: Campo[] }) {
  if (p.versiones.length === 0) {
    return (
      <Text size="sm" c="dimmed">
        Es la primera versión. Cuando cambies sus valores, la de ahora queda guardada aquí.
      </Text>
    )
  }
  // cada versión vieja contra la que le siguió
  const siguiente = (i: number): { capa_mm: number | null; material: string | null; parametros: Parametros } =>
    i === 0 ? p : (p.versiones[i - 1] as VersionPerfil)
  return (
    <Table>
      <Table.Thead>
        <Table.Tr>
          <Table.Th>Versión</Table.Th>
          <Table.Th>Usada</Table.Th>
          <Table.Th>Qué cambió después</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {p.versiones.map((v, i) => (
          <Table.Tr key={v.version}>
            <Table.Td>v{v.version}</Table.Td>
            <Table.Td>
              <Text size="xs">
                {cuando(v.desde)} a {cuando(v.hasta)}
              </Text>
              {v.usuario && (
                <Text size="xs" c="dimmed">
                  cambió {v.usuario}
                </Text>
              )}
            </Table.Td>
            <Table.Td>
              {diferencias(v, siguiente(i), campos).map((d) => (
                <Text key={d} size="xs">
                  {d}
                </Text>
              ))}
            </Table.Td>
          </Table.Tr>
        ))}
      </Table.Tbody>
    </Table>
  )
}

function Archivos({ p }: { p: PerfilFicha }) {
  const refrescar = useRefrescarTaller()
  const [texto, setTexto] = useState('')
  const [buscado] = useDebouncedValue(texto, 300)
  const encontrados = useQuery({
    queryKey: ['buscar-archivos', buscado],
    queryFn: () => api<ArchivoEncontrado[]>(`/archivos/buscar?q=${encodeURIComponent(buscado)}`),
    enabled: buscado.trim().length > 2,
  })
  const ligar = useMutation({
    mutationFn: (archivo: number) =>
      api(`/perfiles/${p.id}/archivos`, { method: 'POST', body: JSON.stringify({ archivo_id: archivo }) }),
    onSuccess: () => {
      setTexto('')
      refrescar()
    },
  })
  const quitar = useMutation({
    mutationFn: (archivo: number) => api(`/perfiles/${p.id}/archivos/${archivo}`, { method: 'DELETE' }),
    onSuccess: refrescar,
  })
  return (
    <Stack gap="xs">
      {p.archivos.length === 0 && (
        <Text size="sm" c="dimmed">
          Sin archivos del laminador.
        </Text>
      )}
      {p.archivos.length > 0 && (
        <Table>
          <Table.Tbody>
            {p.archivos.map((a) => (
              <Table.Tr key={a.id}>
                <Table.Td>
                  <Anchor href={`/api/v1/archivos/${a.id}/contenido`} size="sm" style={{ wordBreak: 'break-all' }}>
                    {a.ruta}
                  </Anchor>
                  {a.estado === 'falta' && (
                    <Badge size="xs" color="red" variant="light" ml={6}>
                      ya no está
                    </Badge>
                  )}
                </Table.Td>
                <Table.Td>
                  <Badge size="xs" variant="light" color={COLOR_TIPO[a.tipo] ?? 'gray'}>
                    {a.ext}
                  </Badge>
                </Table.Td>
                <Table.Td ta="right">
                  <Text size="xs" c="dimmed">
                    {tamano(a.bytes)}
                  </Text>
                </Table.Td>
                <Table.Td ta="right">
                  <Button size="compact-xs" variant="subtle" color="gray" onClick={() => quitar.mutate(a.id)}>
                    Quitar
                  </Button>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      <Select
        label="Ligar un archivo de la colección"
        placeholder="Busca por su ruta: «Printers mono pwsr»…"
        searchable
        searchValue={texto}
        onSearchChange={setTexto}
        filter={({ options }) => options}
        nothingFoundMessage={buscado.trim().length > 2 && !encontrados.isFetching ? 'Nada coincide' : undefined}
        data={(encontrados.data ?? [])
          .filter((a) => !p.archivos.some((x) => x.id === a.id))
          .map((a) => ({ value: String(a.id), label: a.ruta }))}
        value={null}
        onChange={(v) => v && ligar.mutate(Number(v))}
      />
      {(ligar.error ?? quitar.error) && <Alert color="red">{(ligar.error ?? quitar.error)?.message}</Alert>}
    </Stack>
  )
}

function Formulario({ p }: { p: PerfilFicha }) {
  const { impresoras, materiales, campos } = useTaller()
  const refrescar = useRefrescarTaller()
  const inicial = () => ({
    nombre: p.nombre,
    impresora_id: String(p.impresora_id),
    material_id: p.material_id ? String(p.material_id) : null,
    capa_mm: (p.capa_mm ?? '') as number | string,
    parametros: p.parametros,
    en_uso: p.en_uso,
    predeterminado: p.predeterminado,
    notas: p.notas,
  })
  const [f, setF] = useState(inicial)
  const poner = (c: Partial<typeof f>) => setF({ ...f, ...c })
  const tecnologia = impresoras.find((i) => String(i.id) === f.impresora_id)?.tecnologia ?? p.tecnologia
  const tipo = tecnologia === 'fdm' ? 'filamento' : 'resina'
  const valores =
    String(f.material_id) !== String(p.material_id ?? null) ||
    aNumero(f.capa_mm) !== p.capa_mm ||
    JSON.stringify(Object.entries(f.parametros).sort()) !== JSON.stringify(Object.entries(p.parametros).sort())
  const algo =
    valores ||
    f.nombre !== p.nombre ||
    f.impresora_id !== String(p.impresora_id) ||
    f.en_uso !== p.en_uso ||
    f.predeterminado !== p.predeterminado ||
    f.notas !== p.notas
  const guardar = useMutation({
    mutationFn: () =>
      api(`/perfiles/${p.id}`, {
        method: 'PATCH',
        body: JSON.stringify({
          nombre: f.nombre,
          impresora_id: Number(f.impresora_id),
          material_id: f.material_id ? Number(f.material_id) : null,
          capa_mm: aNumero(f.capa_mm),
          parametros: f.parametros,
          en_uso: f.en_uso,
          predeterminado: f.predeterminado,
          notas: f.notas,
        }),
      }),
    onSuccess: refrescar,
  })
  return (
    <Stack gap="sm">
      <TextInput label="Nombre" value={f.nombre} onChange={(e) => poner({ nombre: e.currentTarget.value })} />
      <Group grow align="flex-start">
        <Select
          label="Impresora"
          data={impresoras.filter((i) => i.tecnologia === p.tecnologia).map((i) => ({ value: String(i.id), label: i.nombre }))}
          value={f.impresora_id}
          allowDeselect={false}
          onChange={(v) => v && poner({ impresora_id: v })}
        />
        <Select
          label="Material"
          placeholder="Cualquiera"
          clearable
          data={materiales.filter((m) => m.tipo === tipo).map((m) => ({ value: String(m.id), label: nombreMaterial(m) }))}
          value={f.material_id}
          onChange={(v) => poner({ material_id: v })}
        />
        <NumberInput
          label="Altura de capa"
          suffix=" mm"
          min={0.005}
          max={1}
          step={0.01}
          decimalScale={4}
          value={f.capa_mm}
          onChange={(v) => poner({ capa_mm: v })}
        />
      </Group>
      <CamposPerfil campos={campos} tecnologia={tecnologia} valores={f.parametros} onChange={(v) => poner({ parametros: v })} />
      <Group>
        <Switch label="Lo sigo usando" checked={f.en_uso} onChange={(e) => poner({ en_uso: e.currentTarget.checked })} />
        <Switch
          label="Es el que usa normalmente su impresora"
          checked={f.predeterminado}
          onChange={(e) => poner({ predeterminado: e.currentTarget.checked })}
        />
      </Group>
      <Textarea
        label="Notas"
        placeholder="Para qué funciona: «minis de 32 mm», «bases», qué falló…"
        autosize
        minRows={2}
        value={f.notas}
        onChange={(e) => poner({ notas: e.currentTarget.value })}
      />
      {valores && (
        <Text size="xs" c="blue">
          Cambiaron sus valores: lo de ahora queda guardado como versión {p.version} y este perfil pasa a la {p.version + 1}.
        </Text>
      )}
      {guardar.error && <Alert color="red">{guardar.error.message}</Alert>}
      <Group>
        <Button size="xs" disabled={!algo || !f.nombre.trim()} loading={guardar.isPending} onClick={() => guardar.mutate()}>
          Guardar
        </Button>
        {algo && (
          <Button size="xs" variant="subtle" onClick={() => setF(inicial())}>
            Deshacer cambios
          </Button>
        )}
      </Group>
    </Stack>
  )
}

export default function Perfil() {
  const { id } = useParams()
  const navegar = useNavigate()
  const refrescar = useRefrescarTaller()
  const { campos } = useTaller()
  const q = useQuery({ queryKey: ['perfil', id], queryFn: () => api<PerfilFicha>(`/perfiles/${id}`) })
  const borrar = useMutation({
    mutationFn: () => api(`/perfiles/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      const imp = q.data?.impresora_id
      refrescar()
      navegar(imp ? `/impresoras/${imp}` : '/impresoras?ver=perfiles')
    },
  })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const p = q.data

  return (
    <Stack maw={1000}>
      <Breadcrumbs>
        <Anchor component={Link} to="/impresoras?ver=perfiles">
          Perfiles
        </Anchor>
        <Anchor component={Link} to={`/impresoras/${p.impresora_id}`}>
          {p.impresora}
        </Anchor>
        <Text>{p.nombre}</Text>
      </Breadcrumbs>
      <Group justify="space-between" align="flex-start">
        <div>
          <Group gap="xs">
            <Title order={2}>
              {p.predeterminado && (
                <Text span c="yellow" inherit>
                  ★{' '}
                </Text>
              )}
              {p.nombre}
            </Title>
            <Badge variant="outline" color="gray">
              v{p.version}
            </Badge>
            {p.origen === 'importado' && (
              <Badge variant="light" color="lime">
                importado
              </Badge>
            )}
            {!p.en_uso && (
              <Badge variant="outline" color="gray">
                ya no se usa
              </Badge>
            )}
          </Group>
          <Text size="sm" c="dimmed">
            {p.impresora} · {p.material ?? 'cualquier material'} · actualizado {cuando(p.actualizado)}
          </Text>
        </div>
        <Button size="xs" variant="subtle" color="red" loading={borrar.isPending} onClick={() => borrar.mutate()}>
          Borrar perfil
        </Button>
      </Group>
      {borrar.error && <Alert color="red">{borrar.error.message}</Alert>}

      <Card withBorder>
        <Formulario key={p.actualizado} p={p} />
      </Card>

      <Card withBorder>
        <Text fw={500} mb="xs">
          Archivos del laminador
        </Text>
        <Archivos p={p} />
      </Card>

      <Card withBorder>
        <Text fw={500}>Placas que se imprimen con él</Text>
        <Text size="xs" c="dimmed" mb="xs">
          Las que lo tienen elegido y las que traen un laminado de su impresora con la misma capa y exposición.
        </Text>
        {p.placas.length === 0 ? (
          <Text size="sm" c="dimmed">
            Ninguna todavía.
          </Text>
        ) : (
          <Stack gap={4}>
            {p.placas.map((x) => (
              <Group key={x.id} gap="xs" wrap="nowrap">
                <Anchor component={Link} to={`/placas/${x.id}`} size="sm">
                  {x.nombre}
                </Anchor>
                <Text size="xs" c="dimmed" lineClamp={1}>
                  {x.carpeta}
                </Text>
                <Badge size="xs" variant="outline" color="gray">
                  {x.por === 'elegido' ? 'elegido' : 'por su laminado'}
                </Badge>
              </Group>
            ))}
          </Stack>
        )}
      </Card>

      <Card withBorder>
        <Text fw={500} mb="xs">
          Versiones anteriores
        </Text>
        <Versiones p={p} campos={campos} />
      </Card>
    </Stack>
  )
}
