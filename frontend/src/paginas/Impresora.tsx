import {
  ActionIcon,
  Alert,
  Anchor,
  Badge,
  Breadcrumbs,
  Button,
  Card,
  Group,
  Loader,
  Modal,
  Stack,
  Table,
  Text,
  Title,
  Tooltip,
} from '@mantine/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'

import { api } from '../api'
import { FormImpresora, FormPerfilNuevo } from '../componentes/FormulariosTaller'
import { useRefrescarTaller } from '../opciones'
import Miniatura from '../componentes/Miniatura'
import { COLOR_TIPO, n, resumenImpresora, resumenPerfil, tamano } from '../formato'
import type { DocumentoImpresora, ImpresoraFicha } from '../tipos'

function Documentos({ carpeta, docs }: { carpeta: string; docs: DocumentoImpresora[] }) {
  // agrupados por subcarpeta (Manual, Upgrades/…, Configs…)
  const grupos = new Map<string, DocumentoImpresora[]>()
  for (const d of docs) {
    const sub = d.carpeta === carpeta ? '' : d.carpeta.slice(carpeta.length + 1)
    grupos.set(sub, [...(grupos.get(sub) ?? []), d])
  }
  return (
    <Stack gap="sm">
      {[...grupos.entries()].map(([sub, lista]) => (
        <div key={sub}>
          <Text size="sm" fw={500} c={sub ? undefined : 'dimmed'}>
            {sub || 'En la carpeta'}
          </Text>
          <Table>
            <Table.Tbody>
              {lista.map((d) => (
                <Table.Tr key={d.id}>
                  <Table.Td w={48}>
                    <Miniatura archivoId={d.id} clave={d.miniatura} lado={36} tipo={d.tipo} ext={d.ext} />
                  </Table.Td>
                  <Table.Td>
                    <Anchor href={`/api/v1/archivos/${d.id}/contenido`} size="sm" style={{ wordBreak: 'break-all' }}>
                      {d.nombre}
                    </Anchor>
                  </Table.Td>
                  <Table.Td>
                    <Badge size="xs" variant="light" color={COLOR_TIPO[d.tipo] ?? 'gray'}>
                      {d.tipo}
                    </Badge>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="xs" c="dimmed">
                      {tamano(d.bytes)}
                    </Text>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </div>
      ))}
    </Stack>
  )
}

export default function Impresora() {
  const { id } = useParams()
  const navegar = useNavigate()
  const refrescar = useRefrescarTaller()
  const [editando, setEditando] = useState(false)
  const [nuevoPerfil, setNuevoPerfil] = useState(false)
  const q = useQuery({ queryKey: ['impresora', id], queryFn: () => api<ImpresoraFicha>(`/impresoras/${id}`) })
  const normal = useMutation({
    mutationFn: (perfil: number | null) => api(`/impresoras/${id}`, { method: 'PATCH', body: JSON.stringify({ perfil_id: perfil }) }),
    onSuccess: refrescar,
  })
  const borrar = useMutation({
    mutationFn: () => api(`/impresoras/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      refrescar()
      navegar('/impresoras')
    },
  })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const i = q.data

  return (
    <Stack maw={1000}>
      <Breadcrumbs>
        <Anchor component={Link} to="/impresoras">
          Impresoras y materiales
        </Anchor>
        <Text>{i.nombre}</Text>
      </Breadcrumbs>
      <Group justify="space-between" align="flex-start">
        <div>
          <Group gap="xs">
            <Title order={2}>{i.nombre}</Title>
            <Badge variant="light" color={i.tecnologia === 'fdm' ? 'orange' : 'teal'}>
              {i.tecnologia === 'fdm' ? 'FDM' : 'resina'}
            </Badge>
            {!i.en_uso && (
              <Badge variant="outline" color="gray">
                ya no se usa
              </Badge>
            )}
          </Group>
          <Text c="dimmed" size="sm">
            {[i.marca, i.modelo].filter(Boolean).join(' · ')}
            {i.marca || i.modelo ? ' — ' : ''}
            {resumenImpresora(i)}
            {i.pixel_um ? ` · pixel de ${i.pixel_um} µm` : ''}
            {i.potencia_w ? ` · ${i.potencia_w} W` : ''}
          </Text>
          {i.notas && (
            <Text size="sm" mt={4} style={{ whiteSpace: 'pre-wrap' }}>
              {i.notas}
            </Text>
          )}
        </div>
        <Group gap="xs">
          <Button size="xs" variant="light" onClick={() => setEditando(true)}>
            Editar
          </Button>
          {i.n_perfiles === 0 && (
            <Button size="xs" variant="subtle" color="red" loading={borrar.isPending} onClick={() => borrar.mutate()}>
              Borrar
            </Button>
          )}
        </Group>
      </Group>
      {borrar.error && <Alert color="red">{borrar.error.message}</Alert>}
      <Group gap="xs">
        <Button size="xs" variant="default" component={Link} to={`/placas?impresora_id=${i.id}`}>
          Ver sus {n(i.n_placas)} placas
        </Button>
        {i.tecnologia === 'resina' && !i.res_x && (
          <Text size="xs" c="orange">
            Sin la resolución de la pantalla no se reconocen sus laminados.
          </Text>
        )}
      </Group>

      <Card withBorder>
        <Group justify="space-between" mb="xs">
          <Text fw={500}>Perfiles</Text>
          <Button size="xs" variant="light" onClick={() => setNuevoPerfil(true)}>
            Nuevo perfil
          </Button>
        </Group>
        {i.perfiles.length === 0 ? (
          <Text size="sm" c="dimmed">
            Todavía no tiene perfiles.
          </Text>
        ) : (
          <Table.ScrollContainer minWidth={560}>
            <Table highlightOnHover>
              <Table.Tbody>
                {i.perfiles.map((p) => (
                  <Table.Tr key={p.id} style={{ opacity: p.en_uso ? 1 : 0.55 }}>
                    <Table.Td w={36}>
                      <Tooltip label={p.predeterminado ? 'Es el que usa normalmente' : 'Hacerlo el que usa normalmente'}>
                        <ActionIcon
                          variant="subtle"
                          color={p.predeterminado ? 'yellow' : 'gray'}
                          loading={normal.isPending && normal.variables === p.id}
                          onClick={() => normal.mutate(p.predeterminado ? null : p.id)}
                          aria-label="Perfil normal"
                        >
                          <Text c={p.predeterminado ? 'yellow' : 'dimmed'} inherit>
                            {p.predeterminado ? '★' : '☆'}
                          </Text>
                        </ActionIcon>
                      </Tooltip>
                    </Table.Td>
                    <Table.Td>
                      <Anchor component={Link} to={`/perfiles/${p.id}`} size="sm">
                        {p.nombre}
                      </Anchor>
                      <Text size="xs" c="dimmed">
                        {p.material ?? 'cualquier material'}
                        {p.en_uso ? '' : ' · ya no se usa'}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm">{resumenPerfil(p)}</Text>
                    </Table.Td>
                    <Table.Td ta="right">
                      <Badge size="xs" variant="outline" color="gray">
                        v{p.version}
                      </Badge>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </Table.ScrollContainer>
        )}
      </Card>

      <Card withBorder>
        <Text fw={500}>Documentos</Text>
        <Text size="xs" c="dimmed" mb="xs">
          {i.carpeta ? `Lo que hay en ${i.carpeta}` : 'Pon su carpeta de la colección (Editar) para ver aquí sus manuales y mejoras.'}
        </Text>
        {i.documentos.length > 0 && <Documentos carpeta={i.carpeta} docs={i.documentos} />}
      </Card>

      <Modal opened={editando} onClose={() => setEditando(false)} title={`Editar ${i.nombre}`} size="lg">
        {editando && <FormImpresora inicial={i} onListo={() => setEditando(false)} />}
      </Modal>
      <Modal opened={nuevoPerfil} onClose={() => setNuevoPerfil(false)} title="Nuevo perfil">
        {nuevoPerfil && <FormPerfilNuevo impresoraId={i.id} onListo={(pid) => navegar(`/perfiles/${pid}`)} />}
      </Modal>
    </Stack>
  )
}
