import {
  Alert,
  Anchor,
  Badge,
  Breadcrumbs,
  Card,
  Code,
  Group,
  Loader,
  ScrollArea,
  SegmentedControl,
  SimpleGrid,
  Stack,
  Table,
  Text,
  Title,
  Tooltip,
} from '@mantine/core'
import { useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'
import { Link, useParams } from 'react-router'

import { api } from '../api'
import { COLOR_TIPO, n, tamano, titulo } from '../formato'
import type { ArchivoFila, Ficha } from '../tipos'

function Campo({ nombre, valor }: { nombre: string; valor: string }) {
  return (
    <div>
      <Text size="xs" c="dimmed" tt="uppercase" fw={600}>
        {nombre}
      </Text>
      <Text size="sm">{valor || '—'}</Text>
    </div>
  )
}

/** Ruta del archivo relativa a la carpeta del modelo (o completa si está fuera de ella). */
function relativa(a: ArchivoFila, carpeta: string): string {
  return a.ruta.startsWith(carpeta + '/') ? a.ruta.slice(carpeta.length + 1) : a.ruta
}

export default function Modelo() {
  const { id } = useParams()
  const [tipo, setTipo] = useState('todos')
  const f = useQuery({ queryKey: ['modelo', id], queryFn: () => api<Ficha>(`/modelos/${id}`) })

  const tipos = useMemo(() => {
    const c = new Map<string, number>()
    for (const a of f.data?.archivos ?? []) c.set(a.tipo, (c.get(a.tipo) ?? 0) + 1)
    return [...c.entries()].sort((a, b) => b[1] - a[1])
  }, [f.data])

  if (f.isPending) return <Loader />
  if (f.error) return <Alert color="red">{f.error.message}</Alert>
  const m = f.data
  const archivos = m.archivos.filter((a) => tipo === 'todos' || a.tipo === tipo)
  const MAX = 1000

  return (
    <Stack maw={1100}>
      <Breadcrumbs>
        <Anchor component={Link} to="/catalogo">
          Catálogo
        </Anchor>
        <Anchor component={Link} to={`/catalogo?juego=${encodeURIComponent(m.juego)}`}>
          {m.juego}
        </Anchor>
        {m.faccion && (
          <Anchor
            component={Link}
            to={`/catalogo?juego=${encodeURIComponent(m.juego)}&faccion=${encodeURIComponent(m.faccion)}`}
          >
            {m.faccion}
          </Anchor>
        )}
      </Breadcrumbs>

      <div>
        <Title order={2}>
          {titulo(m)}
          {m.estudio && (
            <Text span c="dimmed" inherit fw={400}>
              {' '}
              ({m.estudio})
            </Text>
          )}
        </Title>
        {m.subcarpeta && <Text c="dimmed">{m.subcarpeta}</Text>}
        <Group gap={4} mt={6}>
          {m.etiquetas.map((e) => (
            <Badge
              key={e}
              variant="light"
              component={Link}
              to={`/catalogo?etiqueta=${encodeURIComponent(e)}`}
              style={{ cursor: 'pointer' }}
            >
              {e}
            </Badge>
          ))}
          {m.revisado && (
            <Badge variant="outline" color={m.revisado === 'No mover' ? 'orange' : 'green'}>
              {m.revisado}
            </Badge>
          )}
        </Group>
      </div>

      <Card withBorder>
        <SimpleGrid cols={{ base: 2, sm: 4 }}>
          <Campo nombre="Archivos" valor={n(m.n_archivos)} />
          <Campo nombre="Mallas" valor={n(m.n_mallas)} />
          <Campo nombre="En el disco" valor={tamano(m.bytes)} />
          <Campo nombre="En frío" valor={m.n_frio ? `${n(m.n_frio)} · ${tamano(m.bytes_frio)}` : ''} />
          <Campo nombre="Confianza" valor={m.confianza} />
        </SimpleGrid>
        <Text size="xs" c="dimmed" mt="md">
          Carpeta
        </Text>
        <Code block>{m.carpeta}</Code>
        {m.notas && (
          <>
            <Text size="xs" c="dimmed" mt="md">
              Notas
            </Text>
            <Text size="sm">{m.notas}</Text>
          </>
        )}
      </Card>

      <Card withBorder>
        <Group justify="space-between" mb="xs" wrap="wrap">
          <Text fw={500}>Archivos</Text>
          {tipos.length > 1 && (
            <ScrollArea type="never">
              <SegmentedControl
                size="xs"
                value={tipo}
                onChange={setTipo}
                data={[
                  { value: 'todos', label: `Todos (${m.archivos.length})` },
                  ...tipos.map(([t, c]) => ({ value: t, label: `${t} (${c})` })),
                ]}
              />
            </ScrollArea>
          )}
        </Group>
        <ScrollArea>
          <Table striped miw={600}>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Archivo</Table.Th>
                <Table.Th miw={72}>Tipo</Table.Th>
                <Table.Th ta="right">Tamaño</Table.Th>
                <Table.Th ta="right">Triángulos</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {archivos.slice(0, MAX).map((a) => (
                <Table.Tr key={a.id} c={a.estado === 'falta' ? 'red' : undefined}>
                  <Table.Td>
                    <Text size="sm" style={{ wordBreak: 'break-all' }}>
                      {relativa(a, m.carpeta)}
                    </Text>
                    <Group gap={4}>
                      {a.estado === 'falta' && (
                        <Badge size="xs" color="red" variant="light">
                          ya no está
                        </Badge>
                      )}
                      {a.nlink > 1 && (
                        <Tooltip label="El mismo archivo tiene otros nombres en el disco (enlace duro)">
                          <Badge size="xs" color="gray" variant="outline">
                            {a.nlink} nombres
                          </Badge>
                        </Tooltip>
                      )}
                    </Group>
                  </Table.Td>
                  <Table.Td miw={72}>
                    <Badge size="xs" variant="light" color={COLOR_TIPO[a.tipo] ?? 'gray'} miw={44}>
                      {a.ext || a.tipo}
                    </Badge>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm">{tamano(a.bytes)}</Text>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm" c="dimmed">
                      {a.triangulos != null ? n(a.triangulos) : a.stl_formato === 'texto' ? 'STL de texto' : ''}
                    </Text>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
        {archivos.length > MAX && (
          <Text size="sm" c="dimmed" mt="xs">
            Se muestran {n(MAX)} de {n(archivos.length)}; filtra por tipo para ver el resto.
          </Text>
        )}
      </Card>

      {m.copias_frio.length > 0 && (
        <Card withBorder>
          <Text fw={500}>Almacenamiento frío</Text>
          <Text size="xs" c="dimmed" mb="xs">
            Copias guardadas fuera del disco
          </Text>
          <ScrollArea>
            <Table striped miw={600}>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Dónde</Table.Th>
                  <Table.Th>Archivo</Table.Th>
                  <Table.Th ta="right">Tamaño</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {m.copias_frio.map((c) => (
                  <Table.Tr key={c.id}>
                    <Table.Td>
                      <Badge variant="light" color="blue">
                        {c.ubicacion}
                      </Badge>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm" style={{ wordBreak: 'break-all' }}>
                        {c.ruta.split('/').pop()}
                      </Text>
                      {c.entradas > 0 && (
                        <Text size="xs" c="dimmed">
                          {n(c.entradas)} archivos adentro
                        </Text>
                      )}
                    </Table.Td>
                    <Table.Td ta="right">
                      <Text size="sm">{tamano(c.bytes)}</Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </ScrollArea>
        </Card>
      )}
    </Stack>
  )
}
